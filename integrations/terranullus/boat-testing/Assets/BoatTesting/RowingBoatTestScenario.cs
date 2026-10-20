using UnityEngine;
using UnityEngine.InputSystem;

// Starts the isolated test already aboard; R recovers the crew after an exit.
[DefaultExecutionOrder(100)]
public sealed class RowingBoatTestScenario : MonoBehaviour
{
    [SerializeField] private RowboatController boat;
    [SerializeField] private PlayerController player;
    [SerializeField] private NpcController[] rowers;
    [SerializeField] private BoatSeat[] rowerSeats;

    public RowboatController Boat => boat;
    public PlayerController Player => player;
    public NpcController[] Rowers => rowers;

    public void Configure(RowboatController testBoat, PlayerController testPlayer, NpcController[] crew, BoatSeat[] seats)
    { boat = testBoat; player = testPlayer; rowers = crew; rowerSeats = seats; }

    private void Start() => ReturnAboard();

    public void ReturnAboard()
    {
        for (int i = 0; i < rowers.Length; i++)
        {
            rowers[i].Stop();
            Seat(rowers[i].Boarding, rowerSeats[i]);
        }
        Seat(player.GetComponent<BoatBoarding>(), boat.CommandSeat);
        PlayerView.Activate(player);
    }

    private static void Seat(BoatBoarding character, BoatSeat target)
    {
        if (character.State == BoatBoarding.BoardingState.Seated && character.CurrentSeat == target) return;
        character.CancelBoarding();
        if (!character.TrySeatImmediately(target, true))
            Debug.LogError($"Could not seat {character.name} at {target.name} in the rowboat test.", character);
    }

    private void Update()
    {
        var keys = Keyboard.current;
        if (keys != null && keys.rKey.wasPressedThisFrame) ReturnAboard();
        if (keys != null && keys.escapeKey.wasPressedThisFrame)
        { Cursor.lockState = CursorLockMode.None; Cursor.visible = true; }
        if (Mouse.current != null && Mouse.current.leftButton.wasPressedThisFrame)
        { Cursor.lockState = CursorLockMode.Locked; Cursor.visible = false; }
    }

    private void OnGUI()
    {
        GUI.Box(new Rect(12, Screen.height - 66, 660, 54),
            "Crewed rowboat test — open water\nW: row forward   A / D: steer   E: leave / board   R: return aboard   Esc: cursor");
    }

    private void OnDestroy()
    { Cursor.lockState = CursorLockMode.None; Cursor.visible = true; }
}
