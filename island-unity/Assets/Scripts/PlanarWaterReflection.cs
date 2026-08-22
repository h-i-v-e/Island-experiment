using UnityEngine;

[DisallowMultipleComponent]
[RequireComponent(typeof(Camera))]
public sealed class PlanarWaterReflection : MonoBehaviour
{
    internal const string TextureName = "_PlanarReflectionTexture";
    internal const string MatrixName = "_PlanarReflectionMatrix";
    internal const string AvailableName = "_PlanarReflectionAvailable";

    private static readonly int ReflectionTextureId = Shader.PropertyToID(TextureName);
    private static readonly int ReflectionMatrixId = Shader.PropertyToID(MatrixName);
    private static readonly int ReflectionAvailableId = Shader.PropertyToID(AvailableName);

    [Tooltip("Transform whose local XZ plane defines sea level.")]
    [SerializeField] private Transform reflectionPlane;

    [Tooltip("Fraction of the viewer resolution used by the reflection camera.")]
    [Range(0.25f, 1f)]
    [SerializeField] private float resolutionScale = 0.5f;

    [Tooltip("Small offset that prevents geometry on the water plane from leaking into the reflection.")]
    [Min(0f)]
    [SerializeField] private float clipPlaneOffset = 0.07f;

    [Tooltip("Layers visible in reflections. The Water layer is always excluded.")]
    [SerializeField] private LayerMask reflectionLayers = ~0;

    private Camera sourceCamera;
    private Camera reflectionCamera;
    private RenderTexture reflectionTexture;
    private bool reflectionTextureUsesHdr;

    public Transform ReflectionPlane => reflectionPlane;
    public Camera ReflectionCamera => reflectionCamera;

    public void Configure(Transform plane)
    {
        reflectionPlane = plane;
    }

    private void OnEnable()
    {
        sourceCamera = GetComponent<Camera>();
        Shader.SetGlobalFloat(ReflectionAvailableId, 0f);
    }

    private void OnDisable()
    {
        ReleaseResources();
    }

    private void OnDestroy()
    {
        ReleaseResources();
    }

    private void OnValidate()
    {
        resolutionScale = Mathf.Clamp(resolutionScale, 0.25f, 1f);
        clipPlaneOffset = Mathf.Max(clipPlaneOffset, 0f);
    }

    private void OnPreCull()
    {
        if (!enabled || reflectionPlane == null)
        {
            Shader.SetGlobalFloat(ReflectionAvailableId, 0f);
            return;
        }

        if (sourceCamera == null)
        {
            sourceCamera = GetComponent<Camera>();
        }
        if (sourceCamera == null || sourceCamera.pixelWidth <= 0 || sourceCamera.pixelHeight <= 0)
        {
            Shader.SetGlobalFloat(ReflectionAvailableId, 0f);
            return;
        }

        EnsureReflectionCamera();
        EnsureReflectionTexture();
        RenderReflection();
    }

    private void EnsureReflectionCamera()
    {
        if (reflectionCamera != null)
        {
            return;
        }

        var reflectionObject = new GameObject("Planar Water Reflection Camera")
        {
            hideFlags = HideFlags.HideAndDontSave,
        };
        reflectionCamera = reflectionObject.AddComponent<Camera>();
        reflectionCamera.enabled = false;
    }

    private void EnsureReflectionTexture()
    {
        var width = Mathf.Max(64, Mathf.RoundToInt(sourceCamera.pixelWidth * resolutionScale));
        var height = Mathf.Max(64, Mathf.RoundToInt(sourceCamera.pixelHeight * resolutionScale));
        var format = sourceCamera.allowHDR
            ? RenderTextureFormat.DefaultHDR
            : RenderTextureFormat.Default;
        if (reflectionTexture != null
            && reflectionTexture.width == width
            && reflectionTexture.height == height
            && reflectionTextureUsesHdr == sourceCamera.allowHDR)
        {
            return;
        }

        ReleaseReflectionTexture();
        reflectionTexture = new RenderTexture(width, height, 24, format)
        {
            name = "Planar Water Reflection",
            hideFlags = HideFlags.DontSave,
            filterMode = FilterMode.Bilinear,
            wrapMode = TextureWrapMode.Clamp,
            antiAliasing = 1,
            useMipMap = false,
            autoGenerateMips = false,
        };
        reflectionTextureUsesHdr = sourceCamera.allowHDR;
        reflectionTexture.Create();
    }

    private void RenderReflection()
    {
        reflectionCamera.CopyFrom(sourceCamera);
        reflectionCamera.enabled = false;
        reflectionCamera.targetTexture = reflectionTexture;
        reflectionCamera.depthTextureMode = DepthTextureMode.None;

        var waterLayer = LayerMask.NameToLayer("Water");
        var waterMask = waterLayer >= 0 ? 1 << waterLayer : 0;
        reflectionCamera.cullingMask = sourceCamera.cullingMask
            & reflectionLayers.value
            & ~waterMask;

        var planePosition = reflectionPlane.position;
        var planeNormal = reflectionPlane.up.normalized;
        var plane = new Vector4(
            planeNormal.x,
            planeNormal.y,
            planeNormal.z,
            -Vector3.Dot(planeNormal, planePosition));
        var reflectionMatrix = CalculateReflectionMatrix(plane);

        var reflectedPosition = reflectionMatrix.MultiplyPoint(sourceCamera.transform.position);
        var reflectedForward = reflectionMatrix.MultiplyVector(sourceCamera.transform.forward);
        var reflectedUp = reflectionMatrix.MultiplyVector(sourceCamera.transform.up);
        reflectionCamera.transform.SetPositionAndRotation(
            reflectedPosition,
            Quaternion.LookRotation(reflectedForward, reflectedUp));
        reflectionCamera.worldToCameraMatrix = sourceCamera.worldToCameraMatrix * reflectionMatrix;

        var clipPlane = CameraSpacePlane(
            reflectionCamera,
            planePosition,
            planeNormal,
            1f);
        reflectionCamera.projectionMatrix = sourceCamera.CalculateObliqueMatrix(clipPlane);

        var previousInvertCulling = GL.invertCulling;
        try
        {
            GL.invertCulling = !previousInvertCulling;
            reflectionCamera.Render();
        }
        finally
        {
            GL.invertCulling = previousInvertCulling;
        }

        var gpuProjection = GL.GetGPUProjectionMatrix(
            reflectionCamera.projectionMatrix,
            true);
        var textureScaleAndOffset = Matrix4x4.identity;
        textureScaleAndOffset.m00 = 0.5f;
        // Render textures run opposite to reflected camera clip-space on the
        // vertical axis. Flip V so the captured scene reads as a mirror image
        // receding away from the waterline instead of an upright duplicate.
        textureScaleAndOffset.m11 = -0.5f;
        textureScaleAndOffset.m22 = 0.5f;
        textureScaleAndOffset.m03 = 0.5f;
        textureScaleAndOffset.m13 = 0.5f;
        textureScaleAndOffset.m23 = 0.5f;
        Shader.SetGlobalTexture(ReflectionTextureId, reflectionTexture);
        Shader.SetGlobalMatrix(
            ReflectionMatrixId,
            textureScaleAndOffset * gpuProjection * reflectionCamera.worldToCameraMatrix);
        Shader.SetGlobalFloat(ReflectionAvailableId, 1f);
    }

    private Vector4 CameraSpacePlane(
        Camera camera,
        Vector3 position,
        Vector3 normal,
        float sideSign)
    {
        var offsetPosition = position + normal * clipPlaneOffset;
        var worldToCamera = camera.worldToCameraMatrix;
        var cameraPosition = worldToCamera.MultiplyPoint(offsetPosition);
        var cameraNormal = worldToCamera.MultiplyVector(normal).normalized * sideSign;
        return new Vector4(
            cameraNormal.x,
            cameraNormal.y,
            cameraNormal.z,
            -Vector3.Dot(cameraPosition, cameraNormal));
    }

    internal static Matrix4x4 CalculateReflectionMatrix(Vector4 plane)
    {
        var matrix = Matrix4x4.identity;
        matrix.m00 = 1f - 2f * plane.x * plane.x;
        matrix.m01 = -2f * plane.x * plane.y;
        matrix.m02 = -2f * plane.x * plane.z;
        matrix.m03 = -2f * plane.w * plane.x;
        matrix.m10 = -2f * plane.y * plane.x;
        matrix.m11 = 1f - 2f * plane.y * plane.y;
        matrix.m12 = -2f * plane.y * plane.z;
        matrix.m13 = -2f * plane.w * plane.y;
        matrix.m20 = -2f * plane.z * plane.x;
        matrix.m21 = -2f * plane.z * plane.y;
        matrix.m22 = 1f - 2f * plane.z * plane.z;
        matrix.m23 = -2f * plane.w * plane.z;
        return matrix;
    }

    private void ReleaseResources()
    {
        Shader.SetGlobalFloat(ReflectionAvailableId, 0f);
        ReleaseReflectionTexture();
        if (reflectionCamera != null)
        {
            DestroyUnityObject(reflectionCamera.gameObject);
            reflectionCamera = null;
        }
    }

    private void ReleaseReflectionTexture()
    {
        if (reflectionTexture == null)
        {
            return;
        }
        reflectionTexture.Release();
        DestroyUnityObject(reflectionTexture);
        reflectionTexture = null;
        reflectionTextureUsesHdr = false;
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
