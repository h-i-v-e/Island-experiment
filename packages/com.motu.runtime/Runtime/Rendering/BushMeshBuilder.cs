using System;
using System.Collections.Generic;
using Motu.Interop;
using Motu.Islands;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Rendering
{
    internal sealed class BushMeshBuilder
    {
        internal const float BranchLodHeight = .35f;

        private readonly List<Vector3> positions = new List<Vector3>();
        private readonly List<Vector3> normals = new List<Vector3>();
        private readonly List<Color> colours = new List<Color>();
        private readonly List<int> triangles = new List<int>();
        private readonly List<Vector3> branchPositions = new List<Vector3>();
        private readonly List<Vector3> branchNormals = new List<Vector3>();
        private readonly List<Color> branchColours = new List<Color>();
        private readonly List<Vector2> branchUvs = new List<Vector2>();
        private readonly List<int> branchTriangles = new List<int>();

        internal static Material CreateMaterial(string name, Texture foliageNoise)
        {
            var shader = MotuShaders.Find("Motu/Planet Forest")
                ?? throw new InvalidOperationException("The shared forest shader is missing.");
            var branchFan = Resources.Load<Texture2D>("Motu/TreeBranchFan")
                ?? throw new InvalidOperationException("The shared bare branch texture is missing.");
            var material = new Material(shader) { name = name };
            material.SetFloat("_ForestKind", 5f);
            material.SetFloat("_Cull", (float)CullMode.Off);
            material.SetTexture("_ForestNoise", foliageNoise);
            material.SetTexture("_TwigCard", branchFan);
            // Centre the fan's stem on the bush root.
            const float rootU = 506.5f / 1024f, rootV = 1.5f / 1024f;
            material.SetTextureOffset("_TwigCard", new Vector2(rootU - .5f, rootV));
            material.SetTextureScale("_TwigCard", new Vector2(1f, 1f - rootV));
            return material;
        }

        internal void AddBush(Vector3 root, float yawDegrees, float scale)
        {
            var prototypes = HybridForestNative.SharedShrubPrototypes;
            var style = IslandRandom.Mix(unchecked((uint)BitConverter.SingleToInt32Bits(yawDegrees)));
            var prototype = prototypes[style % (uint)prototypes.Length];
            var rotation = Quaternion.Euler(0f, yawDegrees, 0f);
            var footprint = new Vector3(scale, 1f, scale);
            var normalScale = new Vector3(1f / scale, 1f, 1f / scale);
            var first = positions.Count;
            var height = 0f;
            for (int i = 0; i < prototype.vertices.Length; i++)
            {
                var vertex = prototype.vertices[i];
                positions.Add(root + rotation * Vector3.Scale(vertex, footprint));
                normals.Add(rotation * Vector3.Scale(prototype.normals[i], normalScale).normalized);
                height = Mathf.Max(height, vertex.y);
            }
            colours.AddRange(prototype.material);
            foreach (var index in prototype.triangles) triangles.Add(first + index);
            AddBareBranches(root, yawDegrees, scale * .8f, height * .88f);
        }

        internal Mesh CreateMesh(string name)
            => CreateMesh(name, positions, normals, colours, triangles);

        internal Mesh CreateBranchMesh(string name)
        {
            var mesh = CreateMesh(name, branchPositions, branchNormals, branchColours, branchTriangles);
            mesh.SetUVs(0, branchUvs);
            return mesh;
        }

        private void AddBareBranches(Vector3 root, float yaw, float scale, float height)
        {
            // The original shrub has six spreading fans around two upright shoots.
            for (var branch = 0; branch < 6; ++branch)
            {
                var tier = branch % 3;
                AddBranchFan(root, yaw + branch * 60f, scale, height * (.55f + tier * .095f),
                    .63f + tier * .025f, .31f + tier * .02f, .15f + tier * .3f);
            }
            AddBranchFan(root, yaw + 17f, scale, height, .14f, .34f, .85f);
            AddBranchFan(root, yaw + 106f, scale, height * .86f, .19f, .37f, .44f);
        }

        private void AddBranchFan(Vector3 root, float yaw, float scale, float height, float spread,
            float halfWidth, float tint)
        {
            var rotation = Quaternion.Euler(0f, yaw, 0f);
            var side = rotation * Vector3.right * (halfWidth * scale);
            var bottom = root - Vector3.up * .04f;
            var top = root + rotation * Vector3.forward * (spread * scale) + Vector3.up * height;
            var normal = Vector3.Cross(side, top - bottom).normalized;
            // A zero green channel tags wood in the shared shrub shader.
            var colour = new Color(tint, 0f, 1f, 1f);
            var first = branchPositions.Count;
            AddVertex(bottom - side, Vector2.zero);
            AddVertex(bottom + side, Vector2.right);
            AddVertex(top + side, Vector2.one);
            AddVertex(top - side, Vector2.up);
            branchTriangles.Add(first);
            branchTriangles.Add(first + 1);
            branchTriangles.Add(first + 2);
            branchTriangles.Add(first);
            branchTriangles.Add(first + 2);
            branchTriangles.Add(first + 3);

            void AddVertex(Vector3 position, Vector2 uv)
            {
                branchPositions.Add(position);
                branchNormals.Add(normal);
                branchColours.Add(colour);
                branchUvs.Add(uv);
            }
        }

        private static Mesh CreateMesh(string name, List<Vector3> positions, List<Vector3> normals,
            List<Color> colours, List<int> triangles)
        {
            var mesh = new Mesh { name = name, indexFormat = positions.Count > 65535 ? IndexFormat.UInt32 : IndexFormat.UInt16 };
            mesh.SetVertices(positions);
            mesh.SetNormals(normals);
            mesh.SetColors(colours);
            mesh.SetTriangles(triangles, 0);
            return mesh;
        }
    }
}
