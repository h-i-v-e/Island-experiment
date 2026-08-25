using UnityEngine;

[DisallowMultipleComponent]
[RequireComponent(typeof(Camera))]
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
