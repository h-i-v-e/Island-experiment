using UnityEngine;

namespace Motu.Rendering
{
    [DisallowMultipleComponent]
    [RequireComponent(typeof(Camera))]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "TreeMeshView")]
    public sealed class TreeMeshView : MonoBehaviour
    {
        public bool IsVisible { get; private set; }

        private void Update()
        {
            if (Input.GetKeyDown(KeyCode.N))
            {
                Toggle();
            }
        }

        public void Toggle()
        {
            IsVisible = !IsVisible;
        }

        private void OnPreRender()
        {
            GL.wireframe = IsVisible;
        }

        private void OnPostRender()
        {
            GL.wireframe = false;
        }

        private void OnDisable()
        {
            GL.wireframe = false;
        }
    }
}
