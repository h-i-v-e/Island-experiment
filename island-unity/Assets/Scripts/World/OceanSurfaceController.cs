using UnityEngine;

[DisallowMultipleComponent]
public sealed class OceanSurfaceController : MonoBehaviour
{
    private const float UnityPlaneSizeMetres = 10f;

    private GameObject surfaceObject;
    private Material surfaceMaterial;

    public Transform SurfaceTransform => surfaceObject != null
        ? surfaceObject.transform
        : null;

    public Material SurfaceMaterial => surfaceMaterial;

    public void Install(
        Material material,
        float diameterMetres,
        bool visible)
    {
        if (material == null)
        {
            throw new System.ArgumentNullException(nameof(material));
        }

        var previousMaterial = surfaceMaterial;
        surfaceMaterial = material;
        EnsureSurfaceObject();
        surfaceObject.transform.localPosition = Vector3.zero;
        surfaceObject.transform.localRotation = Quaternion.identity;
        surfaceObject.transform.localScale = Vector3.one
            * (Mathf.Max(diameterMetres, 1f) / UnityPlaneSizeMetres);
        surfaceObject.GetComponent<MeshRenderer>().sharedMaterial = surfaceMaterial;
        surfaceObject.SetActive(visible);

        if (previousMaterial != null && previousMaterial != surfaceMaterial)
        {
            DestroyUnityObject(previousMaterial);
        }
    }

    public void SetVisible(bool visible)
    {
        surfaceObject?.SetActive(visible);
    }

    private void EnsureSurfaceObject()
    {
        if (surfaceObject != null)
        {
            return;
        }

        surfaceObject = GameObject.CreatePrimitive(PrimitiveType.Plane);
        surfaceObject.name = "Player-Relative Deep Ocean";
        surfaceObject.transform.SetParent(transform, false);
        var waterLayer = LayerMask.NameToLayer("Water");
        if (waterLayer >= 0)
        {
            surfaceObject.layer = waterLayer;
        }
        DestroyUnityObject(surfaceObject.GetComponent<Collider>());
    }

    private void OnDestroy()
    {
        DestroyUnityObject(surfaceMaterial);
        surfaceMaterial = null;
    }

    private static void DestroyUnityObject(Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(value);
        }
        else
        {
            DestroyImmediate(value);
        }
    }
}
