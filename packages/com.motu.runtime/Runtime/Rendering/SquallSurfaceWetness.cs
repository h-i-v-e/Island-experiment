using Motu.World;
using UnityEngine;

namespace Motu.Rendering
{
    [ExecuteAlways, DefaultExecutionOrder(1000), DisallowMultipleComponent, RequireComponent(typeof(Camera))]
    [AddComponentMenu("Motu/Weather/Squall Surface Wetness")]
    public sealed class SquallSurfaceWetness : MonoBehaviour
    {
        [SerializeField] private WorldEnvironmentController environment;
        private SquallWetnessState state;
        private Material material;
        private Camera view;
        internal bool PendingOwnerRemoval { get; set; }
        internal WorldEnvironmentController Environment => environment;
        public float Wetness => state.Amount;
        public float RainIntensity => state.Rain;

        public void Configure(WorldEnvironmentController owner)
        {
            if (environment == owner) return;
            environment = owner;
            state = default;
        }

        private void OnEnable()
        {
            view = GetComponent<Camera>();
            view.depthTextureMode |= DepthTextureMode.Depth;
        }

        private void OnDisable()
        {
            state = default;
            UnityObjectLifetime.DestroyUnityObject(material);
            material = null;
        }

        private void LateUpdate() => Advance(Time.deltaTime);

        internal void Advance(float deltaTime)
        {
            var settings = environment != null ? environment.Weather.Squalls : SquallWeatherSettings.Default;
            var rain = environment != null && environment.IsInstalled && environment.isActiveAndEnabled
                ? environment.SquallRainIntensityAt(transform.position) : 0;
            state.Advance(rain, deltaTime, settings.WettingSeconds, settings.DryingSeconds);
        }

        internal bool NeedsRender => isActiveAndEnabled && state.Amount > .0001f
            && environment != null && environment.Weather.Squalls.SurfaceWetness > 0;

        internal Material PrepareMaterial()
        {
            if (!NeedsRender || PlanarWaterReflection.IsReflectionCamera(view)) return null;
            if (material == null)
            {
                var shader = Resources.Load<Shader>("SquallSurfaceWetness");
                if (shader == null || !shader.isSupported) return null;
                material = CoreMaterial(shader);
            }
            var sea = environment.OceanTransform;
            material.SetVector("_MotuWetness", new Vector4(
                state.Amount * environment.Weather.Squalls.SurfaceWetness, state.Rain, Time.time,
                sea != null ? sea.position.y : 0));
            material.SetFloat("_MotuWetFogEnabled", RenderSettings.fog ? 1 : 0);
            material.SetColor("_MotuWetAmbient", RenderSettings.ambientLight);
            return material;
        }

        private static Material CoreMaterial(Shader shader) => new Material(shader)
        {
            name = "Motu squall surface wetness", hideFlags = HideFlags.HideAndDontSave
        };
    }
}
