using UnityEngine;

namespace Motu.Rendering
{
    internal sealed class MotuShaderLibrary : ScriptableObject
    {
        [SerializeField] private Shader[] shaders;
        internal Shader[] Shaders => shaders;
    }
}
