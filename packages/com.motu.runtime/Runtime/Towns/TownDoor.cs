using UnityEngine;

namespace Motu.Towns
{
    /// <summary>Native-authored door hinge and swing; usable by a host interaction raycast.</summary>
    public sealed class TownDoor : MonoBehaviour
    {
        [SerializeField] private float closedDegrees, swingDegrees;
        [SerializeField] private string doorId;
        [SerializeField] private bool isOpen;
        public bool IsOpen => isOpen;
        public string DoorId => doorId;

        internal void Initialize(TownDoorData data)
        {
            doorId = data.id;
            closedDegrees = -data.closed_angle * Mathf.Rad2Deg;
            swingDegrees = -data.swing_angle * Mathf.Rad2Deg;
            SetOpen(false);
        }

        public void Toggle() => SetOpen(!IsOpen);
        public void SetOpen(bool open)
        {
            isOpen = open;
            transform.localRotation = Quaternion.Euler(0f, closedDegrees + (open ? swingDegrees : 0f), 0f);
        }

        private void OnMouseDown() => Toggle();
    }
}
