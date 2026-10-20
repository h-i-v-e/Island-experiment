using UnityEngine;

// Owns spawned crew and the physics body that FloatingShip detaches at launch.
[DisallowMultipleComponent]
public sealed class RowboatCrew : MonoBehaviour
{
    private Rigidbody hull;
    private NpcController[] rowers;
    public NpcController[] Rowers => rowers;
    public Rigidbody Hull => hull;
    public void Configure(Rigidbody body, NpcController[] crew) { hull = body; rowers = crew; }
    private void OnDestroy()
    {
        if (!Application.isPlaying) return;
        if (rowers != null)
            foreach (var rower in rowers) if (rower != null) Destroy(rower.gameObject);
        if (hull != null && !hull.transform.IsChildOf(transform)) Destroy(hull.gameObject);
    }
}
