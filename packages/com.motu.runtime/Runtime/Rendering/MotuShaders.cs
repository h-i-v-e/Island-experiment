using System;
using UnityEngine;

namespace Motu.Rendering
{
    // Serialized shader references survive player stripping without editing host GraphicsSettings.
    internal static class MotuShaders
    {
        private static MotuShaderLibrary library;
        internal static Shader Find(string name)
        {
            if (library == null) library = Resources.Load<MotuShaderLibrary>("Motu/ShaderLibrary");
            if (library != null)
                foreach (var shader in library.Shaders)
                    if (shader != null && shader.name == name) return shader;
            return Shader.Find(name);
        }
    }
}
