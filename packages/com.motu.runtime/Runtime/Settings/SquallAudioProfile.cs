using UnityEngine;

namespace Motu.Settings
{
    public enum SquallThunderBand { Near, Medium, Far }

    /// <summary>Save as Resources/SquallAudio to supply distant storm thunder.</summary>
    [CreateAssetMenu(menuName = "Motu/Audio/Squall Audio", fileName = "SquallAudio")]
    public sealed class SquallAudioProfile : ScriptableObject
    {
        [HideInInspector] [SerializeField] private AudioClip thunder;
        [Tooltip("Thunder recordings used when the listener is inside the emitting storm's live rain core. The bank is selected when sound arrives, after the distance delay. Clips vary without an immediate repeat when alternatives exist; an empty bank uses the legacy fallback clip.")]
        [SerializeField] private AudioClip[] nearThunder;
        [Tooltip("Thunder recordings used outside the emitting storm's core but within half its Visibility Distance. With a 4 km visibility range, this covers listeners outside the storm up to 2 km from its centre. Selection occurs on sound arrival, not at the instant of the flash.")]
        [SerializeField] private AudioClip[] mediumThunder;
        [Tooltip("Thunder recordings used outside the core at or beyond half the storm's Visibility Distance. With 4 km visibility, the far bank starts at 2 km. This threshold chooses the character of the sound; Maximum Distance separately controls whether it can be heard.")]
        [SerializeField] private AudioClip[] farThunder;
        [Tooltip("Overall thunder gain (0-1), before storm strength and 3D distance attenuation. Applies to every thunder bank. Use this to balance thunder against other game audio without changing the hearing range or strike frequency.")]
        [Range(0f, 1f)] [SerializeField] private float volume = .8f;
        [Tooltip("Distance from the lightning strike, in metres, within which thunder retains full distance gain. Beyond this near plateau the custom attenuation curve fades it out. This is unrelated to the near/medium/far recording-bank boundaries.")]
        [Min(1f)] [SerializeField] private float minimumDistance = 50f;
        [Tooltip("Maximum thunder hearing distance from the fixed strike position, in metres. Sound fades to zero here. The custom curve targets 75% distance gain at 1 km and 50% at 2 km when those points lie inside the range; the default cutoff is 4 km. Bank selection still uses storm Visibility Distance.")]
        [Min(100f)] [SerializeField] private float maximumDistance = 4000f;
        [Tooltip("Propagation speed used to delay thunder after lightning, in metres per second. The default 343 m/s gives about 2.9 seconds of delay per kilometre. Listener movement is accounted for while the sound travels; this changes arrival time, not recording pitch.")]
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
