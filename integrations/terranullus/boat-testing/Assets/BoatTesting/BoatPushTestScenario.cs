using UnityEngine;
using UnityEngine.InputSystem;

[DefaultExecutionOrder(100)]
public sealed class BoatPushTestScenario : MonoBehaviour
{
    [SerializeField] private RowboatController boat;
    [SerializeField] private PlayerController player;
    [SerializeField] private Transform boatStart, playerStart;
    public RowboatController Boat => boat;
    public PlayerController Player => player;

    public void Configure(RowboatController testBoat, PlayerController testPlayer, Transform boatSpawn, Transform playerSpawn)
    { boat = testBoat; player = testPlayer; boatStart = boatSpawn; playerStart = playerSpawn; }

    private void Start() => PlayerView.Activate(player);

    public void ResetTest()
    {
        player.GetComponent<BoatPushing>().CancelPush();
        player.GetComponent<BoatBoarding>().CancelBoarding();
        boat.BeginShorePush();
        var body = boat.Body;
        body.position = boatStart.position; body.rotation = boatStart.rotation;
        body.linearVelocity = body.angularVelocity = Vector3.zero;
        body.transform.SetPositionAndRotation(body.position, body.rotation);
        boat.transform.SetPositionAndRotation(body.position, body.rotation);
        boat.EndShorePush();
        var capsule = player.GetComponent<CharacterController>();
        capsule.enabled = false;
        float yaw = player.transform.eulerAngles.y;
        player.transform.SetPositionAndRotation(playerStart.position, playerStart.rotation);
        player.CarryBoatFacing(Mathf.DeltaAngle(yaw, playerStart.eulerAngles.y));
        player.ResetBoatSupport();
        capsule.enabled = true;
        Physics.SyncTransforms();
        PlayerView.Activate(player);
    }

    private void Update()
    {
        var keys = Keyboard.current;
        if (keys != null && keys.rKey.wasPressedThisFrame) ResetTest();
        if (keys != null && keys.escapeKey.wasPressedThisFrame)
        { Cursor.lockState = CursorLockMode.None; Cursor.visible = true; }
        if (Mouse.current != null && Mouse.current.leftButton.wasPressedThisFrame)
        { Cursor.lockState = CursorLockMode.Locked; Cursor.visible = false; }
    }

    private void OnGUI()
    {
        GUI.Box(new Rect(12, 12, 610, 76), "Boat push test — calm sea and sloping beach\n"
            + "WASD: walk   E: push grounded boat / board afloat boat   R: reset test\n"
            + "Launch seats you at the controls. E or movement cancels pushing. Esc: cursor");
    }
    private void OnDestroy() { Cursor.lockState = CursorLockMode.None; Cursor.visible = true; }
}
