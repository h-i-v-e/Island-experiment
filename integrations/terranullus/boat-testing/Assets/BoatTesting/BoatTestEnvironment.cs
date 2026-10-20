using Motu.Settings;
using Motu.World;
using UnityEngine;

// A standalone ocean; deliberately has no island generator or streaming world.
[DefaultExecutionOrder(-1000)]
public sealed class BoatTestEnvironment : MonoBehaviour
{
    [SerializeField] private WorldEnvironmentConfiguration configuration;
    [SerializeField] private Camera playerCamera;
    private WorldEnvironmentController environment;

    private void Awake()
    {
        environment = gameObject.AddComponent<WorldEnvironmentController>();
        environment.Initialize(configuration.Environment, configuration.Clouds, 4000f, 16000f, playerCamera.transform);
        environment.BindReflectionCamera(playerCamera);
        environment.SetFirstPersonViewActive(true);
    }

    private void OnDisable()
    {
        if (environment != null) environment.enabled = false;
    }

    private void OnEnable()
    {
        if (environment != null) environment.enabled = true;
    }
}
