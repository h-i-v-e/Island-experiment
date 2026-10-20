using System;
using UnityEngine;

// Baked from the humanoid entry/exit clips on the project's reference avatar.
[CreateAssetMenu(menuName = "Boats/Boarding Animation Profile")]
public sealed class BoatBoardingProfile : ScriptableObject
{
    [Serializable]
    public sealed class Motion
    {
        public AnimationClip clip;
        public string state;
        public Vector3[] positions = Array.Empty<Vector3>();
        public Quaternion[] rotations = Array.Empty<Quaternion>();
        public bool IsValid => clip != null && positions.Length > 1 && rotations.Length == positions.Length;

        public Pose Evaluate(float normalizedTime, float scale)
        {
            float sample = Mathf.Clamp01(normalizedTime) * (positions.Length - 1);
            int first = Mathf.Min(Mathf.FloorToInt(sample), positions.Length - 2);
            float blend = sample - first;
            return new Pose(Vector3.Lerp(positions[first], positions[first + 1], blend) * scale,
                Quaternion.Slerp(rotations[first], rotations[first + 1], blend));
        }
    }

    public float referenceHumanScale = 1f;
    [Tooltip("Seated hips relative to the character root, normalized by avatar human scale.")]
    public Vector3 seatedHips;
    public Motion shoreLeft = new Motion(), shoreRight = new Motion();
    public Motion waterLeft = new Motion(), waterRight = new Motion();
    public Motion exitLeft = new Motion(), exitRight = new Motion();

    public Motion Entry(bool port, bool water) => water ? (port ? waterLeft : waterRight) : (port ? shoreLeft : shoreRight);
    public Motion Exit(bool port) => port ? exitLeft : exitRight;

    public static Pose Multiply(Pose parent, Pose local) => new Pose(parent.position + parent.rotation * local.position, parent.rotation * local.rotation);
    public static Pose Inverse(Pose pose)
    {
        var rotation = Quaternion.Inverse(pose.rotation);
        return new Pose(rotation * -pose.position, rotation);
    }
}
