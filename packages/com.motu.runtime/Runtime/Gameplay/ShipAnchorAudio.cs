using UnityEngine;

namespace Motu.Gameplay
{
    [AddComponentMenu("Motu/Ship Anchor Audio")]
    [RequireComponent(typeof(ShipAnchor))]
    [DisallowMultipleComponent]
    public sealed class ShipAnchorAudio : MonoBehaviour
    {
        [SerializeField] private AudioClip dropClip;
        [SerializeField] private AudioClip bottomContactClip;
        [SerializeField] private AudioClip tensionClip;
        [Range(0f, 1f)] [SerializeField] private float volume = .8f;

        private ShipAnchor anchor;
        private AudioSource dropSource, bottomSource, tensionSource;

        private void OnEnable()
        {
            anchor = GetComponent<ShipAnchor>();
            anchor.Dropped += PlayDrop;
            anchor.BottomContact += PlayBottomContact;
            anchor.Tensioned += PlayTension;
        }

        private void PlayDrop() => Play(ref dropSource, "Anchor dropping", dropClip, anchor.BowWorldPosition, 1f);
        private void PlayBottomContact() => Play(ref bottomSource, "Anchor seabed contact", bottomContactClip, anchor.AnchorPosition, .85f);
        private void PlayTension() => Play(ref tensionSource, "Anchor rode holding", tensionClip, anchor.BowWorldPosition, .9f);

        private void Play(ref AudioSource source, string sourceName, AudioClip clip, Vector3 position, float pitch)
        {
            if (clip == null || volume <= 0f) return;
            if (source == null)
            {
                var child = new GameObject(sourceName);
                child.transform.SetParent(transform, false);
                source = child.AddComponent<AudioSource>();
                source.playOnAwake = false;
                source.spatialBlend = 1f;
                source.dopplerLevel = 0f;
                source.minDistance = 16f;
                source.maxDistance = 100f;
                source.rolloffMode = AudioRolloffMode.Linear;
            }
            // Independent voices keep a shallow-water bottom strike from
            // interrupting the sound of paying out the rode on the same frame.
            source.Stop();
            source.transform.position = position;
            source.clip = clip;
            source.volume = volume;
            source.pitch = pitch;
            source.Play();
        }

        private void LateUpdate()
        {
            if (dropSource != null) dropSource.transform.position = anchor.BowWorldPosition;
            if (tensionSource != null) tensionSource.transform.position = anchor.BowWorldPosition;
            if (bottomSource != null) bottomSource.transform.position = anchor.AnchorPosition;
        }

        private void OnDisable()
        {
            if (anchor != null)
            {
                anchor.Dropped -= PlayDrop;
                anchor.BottomContact -= PlayBottomContact;
                anchor.Tensioned -= PlayTension;
            }
            if (dropSource != null) dropSource.Stop();
            if (bottomSource != null) bottomSource.Stop();
            if (tensionSource != null) tensionSource.Stop();
        }
    }
}
