using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class BoatSupportDiagnostics
{
    [MenuItem("Island/Boat Tests/Capture Player Support And Colliders")]
    public static void CaptureCurrent() => Capture("/tmp/boat-support-main.txt");

    private static string Path(Transform item) => item == null ? "none" : item.parent == null ? item.name : Path(item.parent) + "/" + item.name;

    private static object Field(object target, string name) => target.GetType().GetField(name, BindingFlags.Instance | BindingFlags.NonPublic)?.GetValue(target);

    public static void Capture(string reportPath)
    {
        var text = new StringBuilder();
        text.AppendLine($"Playing={Application.isPlaying}, frame={Time.frameCount}, gravity={Physics.gravity:F3}");
        foreach (var player in Object.FindObjectsByType<PlayerController>(FindObjectsInactive.Include))
        {
            if (!player.UsesPlayerInput) continue;
            var capsule = player.GetComponent<CharacterController>();
            var animator = player.GetComponent<Animator>();
            var boarding = player.GetComponent<BoatBoarding>();
            text.AppendLine($"PLAYER {Path(player.transform)} root={player.transform.position:F4}, motor={player.enabled}, capsule={capsule.enabled}, grounded={capsule.isGrounded}, bounds={capsule.bounds}, scale={player.transform.lossyScale}");
            text.AppendLine($" state={boarding.State}, seat={boarding.CurrentSeat?.name}, seatIK={boarding.SeatIKBlend}, airborne={Field(player, "airborne")}, verticalSpeed={Field(player, "verticalSpeed")}, support={Path(Field(player, "ground") as Transform)}, constraint={player.IKConstraint}, animator={animator.enabled}, speed={animator.speed}");
            if (capsule.enabled)
            {
                var origin = capsule.bounds.center + Vector3.up * 2;
                foreach (var hit in Physics.RaycastAll(origin, Vector3.down, 20, Physics.DefaultRaycastLayers, QueryTriggerInteraction.Ignore).OrderBy(h => h.distance))
                    text.AppendLine($" BELOW {Path(hit.transform)} point={hit.point:F4}, normal={hit.normal:F3}, ignored={Physics.GetIgnoreCollision(capsule, hit.collider)}");
                foreach (var shape in Physics.OverlapBox(capsule.bounds.center, capsule.bounds.extents + Vector3.one * .1f, Quaternion.identity))
                    if (shape != capsule) text.AppendLine($" NEAR {Path(shape.transform)} type={shape.GetType().Name}, ignored={Physics.GetIgnoreCollision(capsule, shape)}, bounds={shape.bounds}");
            }
        }
        foreach (var shape in Object.FindObjectsByType<Collider>(FindObjectsInactive.Include))
        {
            text.AppendLine($"COLLIDER {Path(shape.transform)} type={shape.GetType().Name} enabled={shape.enabled}, active={shape.gameObject.activeInHierarchy}, trigger={shape.isTrigger}, bounds={shape.bounds}, body={shape.attachedRigidbody?.name}");
            if (shape is MeshCollider mesh) text.AppendLine($" mesh={mesh.sharedMesh?.name}, convex={mesh.convex}, meshBounds={mesh.sharedMesh?.bounds}");
        }
        File.WriteAllText(reportPath, text.ToString());
    }
}
