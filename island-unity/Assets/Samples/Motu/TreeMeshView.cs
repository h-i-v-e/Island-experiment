using UnityEngine;
using UnityEngine.Rendering;

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

        private void OnEnable()
        {
            RenderPipelineManager.beginCameraRendering += BeginCamera;
            RenderPipelineManager.endCameraRendering += EndCamera;
        }

        private void BeginCamera(ScriptableRenderContext context, Camera camera)
        {
            if (camera == GetComponent<Camera>()) GL.wireframe = IsVisible;
        }

        private void EndCamera(ScriptableRenderContext context, Camera camera)
        {
            GL.wireframe = false;
        }

        private void OnDisable()
        {
            RenderPipelineManager.beginCameraRendering -= BeginCamera;
            RenderPipelineManager.endCameraRendering -= EndCamera;
            GL.wireframe = false;
        }
    }
}
