using UnityEngine;

namespace Motu.Settings
{
    public enum SquallThunderBand { Near, Medium, Far }

    /// <summary>Save as Resources/SquallAudio to supply distant storm thunder.</summary>
    [CreateAssetMenu(menuName = "Motu/Audio/Squall Audio", fileName = "SquallAudio")]
    public sealed class SquallAudioProfile : ScriptableObject
    {
        [HideInInspector] [SerializeField] private AudioClip thunder;
        [SerializeField] private AudioClip[] nearThunder;
        [SerializeField] private AudioClip[] mediumThunder;
        [SerializeField] private AudioClip[] farThunder;
        [Range(0f, 1f)] [SerializeField] private float volume = .8f;
        [Min(1f)] [SerializeField] private float minimumDistance = 50f;
        [Min(100f)] [SerializeField] private float maximumDistance = 4000f;
        [Min(1f)] [SerializeField] private float soundSpeedMetresPerSecond = 343f;

        public bool HasThunder => thunder != null || HasClips(nearThunder) || HasClips(mediumThunder) || HasClips(farThunder);
        public float Volume => Mathf.Clamp01(volume);
        public float MinimumDistance => Mathf.Max(1f, minimumDistance);
        public float MaximumDistance => Mathf.Max(MinimumDistance + 1f, maximumDistance);
        public float SoundSpeedMetresPerSecond => Mathf.Max(1f, soundSpeedMetresPerSecond);

        public AudioClip SelectThunder(SquallThunderBand band, uint seed, AudioClip previous)
        {
            var clips = band == SquallThunderBand.Near ? nearThunder
                : band == SquallThunderBand.Medium ? mediumThunder : farThunder;
            if (clips == null || clips.Length == 0) return thunder;
            var start = (int)(seed % (uint)clips.Length);
            AudioClip fallback = thunder;
            for (var i = 0; i < clips.Length; i++)
            {
                var clip = clips[(start + i) % clips.Length];
                if (clip == null) continue;
                fallback = clip;
                if (clip != previous) return clip;
            }
            return fallback;
        }

        private static bool HasClips(AudioClip[] clips)
        {
            if (clips == null) return false;
            foreach (var clip in clips) if (clip != null) return true;
            return false;
        }
    }
}
