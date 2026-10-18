using System;
using System.Reflection;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanShoreAudioTests
    {
        internal sealed class Coast : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Synthetic audible coast");
            internal readonly OceanSurfaceController Ocean;
            internal readonly Material Material;
            internal readonly Texture2D Depth = new Texture2D(256, 256, TextureFormat.RGBAFloat, false, true);
            internal readonly Texture2D Direction = new Texture2D(256, 256, TextureFormat.RGBAFloat, false, true);
            readonly Vector4 oldWind;
            internal Coast()
            {
                Assert.IsTrue(OceanShoreSampler.Supported);
                var shader=Resources.Load<Shader>("OceanShoreQuery");Assert.IsNotNull(shader);
                Material=new Material(shader);
                Material.SetFloat("_OnshoreWaveEnabled",1);
                Material.SetFloat("_GeometricWaves",1);
                Material.SetVector("_OnshoreWaveParameters",new Vector4(12,.5f,2.2f,.18f));
                Material.SetVector("_OnshoreWaveBreaking",new Vector4(.95f,96,5,3.5f));
                Material.SetVector("_WaveAttenuationWorldRect",new Vector4(-128,-128,1f/256,1f/256));
                Depth.wrapMode=Direction.wrapMode=TextureWrapMode.Clamp;
                Depth.filterMode=Direction.filterMode=FilterMode.Bilinear;
                Material.SetTexture("_WaveAttenuationTex",Depth);Material.SetTexture("_WaveOnshoreTex",Direction);
                oldWind=Shader.GetGlobalVector("_MotuWeatherWind");
                Shader.SetGlobalVector("_MotuWeatherWind",new Vector4(1,0,9,1));
                Fill();Ocean=Root.AddComponent<OceanSurfaceController>();
                const BindingFlags flags=BindingFlags.Instance|BindingFlags.NonPublic;
                typeof(OceanSurfaceController).GetField("surfaceMaterial",flags).SetValue(Ocean,Material);
                var surface=new GameObject("Sea level");surface.transform.SetParent(Root.transform);
                typeof(OceanSurfaceController).GetField("surfaceObject",flags).SetValue(Ocean,surface);
            }
            internal void Fill(bool river=false,float uniformDepth=-1,float slope=.2f,
                bool retainOnshoreInRiver=false,float riverAllowance=1f)
            {
                var depth=new Color[256*256];var direction=new Color[depth.Length];
                for(int z=0;z<256;z++)for(int x=0;x<256;x++) {
                    float distance=x+.5f-128;
                    float water=uniformDepth>=0?uniformDepth:Mathf.Clamp(distance*slope,0,10);
                    depth[z*256+x]=new Color(1,Mathf.Clamp01(distance/128),water/10,river?0:riverAllowance);
                    direction[z*256+x]=new Color(0,.5f,river&&!retainOnshoreInRiver?0:1,Mathf.Clamp01(distance/128));
                }
                Depth.SetPixels(depth);Depth.Apply();Direction.SetPixels(direction);Direction.Apply();
            }
            public void Dispose()
            {
                Shader.SetGlobalVector("_MotuWeatherWind",oldWind);
                Object.DestroyImmediate(Root);Object.DestroyImmediate(Depth);Object.DestroyImmediate(Direction);
            }
        }

        [TestCase(-10f)]
        [TestCase(20.125f)]
        [TestCase(50f)]
        public void PlacementFindsAndSpreadsAcrossBreakingDepthFromLandContourAndSea(float playerX)
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(48);
            Assert.IsTrue(query.SubmitPlacement(coast.Ocean,new Vector3(playerX,0,0),80));
            Assert.IsFalse(query.SubmitPlacement(coast.Ocean,Vector3.zero,80),"Only one readback may be in flight");
            AsyncGPUReadback.WaitAllRequests();Assert.That(query.Revision,Is.EqualTo(1));
            int hits=0,preferred=0;float minZ=1000,maxZ=-1000;
            for(int i=0;i<48;i++)if(query.TryGetSample(i,out var hit)) {
                hits++;Assert.That(hit.Depth,Is.InRange(.15f,5f));
                if(hit.Influence>.75f) {
                    preferred++;Assert.That(hit.Depth,Is.EqualTo(4.025f).Within(.015f));
                    Assert.That(hit.Position.x,Is.EqualTo(20.125f).Within(.08f));
                }
                Assert.That(Vector3.Distance(hit.Position,new Vector3(playerX,0,0)),Is.LessThan(80.1f));
                minZ=Mathf.Min(minZ,hit.Position.z);maxZ=Mathf.Max(maxZ,hit.Position.z);
            }
            Assert.That(hits,Is.GreaterThan(12));Assert.That(preferred,Is.GreaterThan(12));Assert.That(maxZ-minZ,Is.GreaterThan(100));
            Assert.IsFalse(ShaderUtil.ShaderHasError(coast.Material.shader));
        }

        [TestCase(true,-1f,true)]
        [TestCase(false,0f,true)]
        [TestCase(false,10f,true)]
        [TestCase(false,-1f,false)]
        public void PlacementRejectsRiversDryLandDeepWaterAndDisabledShoreWaves(bool river,float depth,bool enabled)
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(48);
            coast.Fill(river,depth);coast.Material.SetFloat("_OnshoreWaveEnabled",enabled?1:0);
            query.SubmitPlacement(coast.Ocean,new Vector3(-10,0,0),80);AsyncGPUReadback.WaitAllRequests();
            for(int i=0;i<48;i++)Assert.IsFalse(query.TryGetSample(i,out _));
        }

        [Test]
        public void GentleBeachUsesShallowerBreakingWaterInsideTheAudibleBand()
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(48);
            coast.Fill(slope:.02f);
            query.SubmitPlacement(coast.Ocean,new Vector3(-10,0,0),80);AsyncGPUReadback.WaitAllRequests();
            int hits=0;
            for(int i=0;i<48;i++)if(query.TryGetSample(i,out var sample))
            { hits++;Assert.That(sample.Depth,Is.InRange(.15f,1.6f));Assert.That(sample.Position.x,Is.LessThan(77)); }
            Assert.That(hits,Is.GreaterThan(6));
        }

        [TestCase(-1f,.2f)]
        [TestCase(-1f,.02f)]
        [TestCase(4f,.2f)]
        public void LocalRiverMaskRejectsProbesEvenWhenTheDirectionFieldBleedsIntoSubmergedChannels(float depth,float slope)
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(48);
            // Unlike the original river fixture, leave the shore direction/influence
            // fully enabled. The local carve mask must reject this water by itself.
            coast.Fill(river:true,uniformDepth:depth,slope:slope,retainOnshoreInRiver:true);
            query.SubmitPlacement(coast.Ocean,new Vector3(20,0,0),80);AsyncGPUReadback.WaitAllRequests();
            for(int i=0;i<48;i++)Assert.IsFalse(query.TryGetSample(i,out _));
        }

        [Test]
        public void ExistingWaveProbeStopsReportingBreakersWhenLocalRiverMaskArrives()
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(1);
            var positions=new[]{new Vector3(20.125f,0,0)};
            query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
            Assert.IsTrue(query.TryGetSample(0,out _));
            coast.Fill(river:true,uniformDepth:4,retainOnshoreInRiver:true);
            for(int i=0;i<32;i++) {
                coast.Material.SetFloat("_OnshoreWavePhase",2*Mathf.PI*i/32);
                query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
                Assert.IsFalse(query.TryGetSample(0,out _),"A retained submerged river probe must stay silent throughout the wave cycle");
            }
        }

        [TestCase(.5f,1f,4f,1f)]
        [TestCase(.25f,1f,4f,1f)]
        [TestCase(.5f,.5f,4f,1f)]
        [TestCase(.5f,1f,4f,.06f)]
        [TestCase(5f,1f,.5f,1f)]
        public void LocalAmplitudeFollowsWaveSizeWindDepthAndRiverAttenuationWithoutDependingOnTriggerPhase(
            float amplitude,float wind,float depth,float riverAllowance)
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(1);
            coast.Fill(uniformDepth:depth,riverAllowance:riverAllowance);
            coast.Material.SetVector("_OnshoreWaveParameters",new Vector4(12,amplitude,2.2f,.18f));
            Shader.SetGlobalVector("_MotuWeatherWind",new Vector4(1,0,9,wind));
            var positions=new[]{new Vector3(20.125f,0,0)};
            float riverBlend=Mathf.SmoothStep(0,1,Mathf.InverseLerp(.02f,.10f,riverAllowance));
            float expected=Mathf.Min(amplitude*wind,depth/(1+.22f*.18f))*riverBlend;
            for(int i=0;i<16;i++) {
                coast.Material.SetFloat("_OnshoreWavePhase",2*Mathf.PI*i/16);
                query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
                Assert.IsTrue(query.TryGetSample(0,out var sample));
                Assert.That(sample.Amplitude,Is.EqualTo(expected).Within(.001f));
            }
            coast.Material.SetFloat("_GeometricWaves",0);
            query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
            Assert.IsTrue(query.TryGetSample(0,out var disabled));
            Assert.That(disabled.Amplitude,Is.Zero);Assert.That(disabled.Breaking,Is.Zero);
        }

        [Test]
        public void ShoreCrestPassesOncePerCycleAndOrdinarySwellCannotTriggerIt()
        {
            using var coast=new Coast();using var query=new OceanShoreSampler(1);
            var positions=new[]{new Vector3(20.125f,0,0)};
            float maximum=0,minimum=1;int crests=0;bool latched=false;
            for(int i=0;i<=128;i++) {
                float phase=2*Mathf.PI*i/64;
                coast.Material.SetFloat("_OnshoreWavePhase",phase);
                query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
                Assert.IsTrue(query.TryGetSample(0,out var sample));
                maximum=Mathf.Max(maximum,sample.Breaking);minimum=Mathf.Min(minimum,sample.Breaking);
                if(sample.Breaking<=.02f)latched=false;
                if(!latched && sample.Breaking>=.08f) { crests++;latched=true; }
                // Offshore wave phase/amplitude have no ownership of the breaker signal.
                coast.Material.SetVector("_OceanWave0",new Vector4(1,0,30,30));
                coast.Material.SetVector("_OceanWaveFrom0",new Vector4(1,0,30,phase*3));
                query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
                Assert.IsTrue(query.TryGetSample(0,out var changed));
                Assert.That(changed.Breaking,Is.EqualTo(sample.Breaking).Within(.0001f));
            }
            Assert.That(maximum,Is.GreaterThan(.5f));Assert.That(minimum,Is.LessThan(.001f));
            Assert.That(crests,Is.EqualTo(2));
            Shader.SetGlobalVector("_MotuWeatherWind",new Vector4(1,0,0,0));
            query.SubmitWaves(coast.Ocean,positions);AsyncGPUReadback.WaitAllRequests();
            Assert.IsTrue(query.TryGetSample(0,out var calm));Assert.That(calm.Breaking,Is.EqualTo(0));
        }
    }
}
