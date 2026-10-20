using UnityEngine;

namespace Motu.Settings
{
    /// <summary>Save as Resources/WaterfallAudio to configure generated waterfall loops.</summary>
    [CreateAssetMenu(menuName = "Motu/Audio/Waterfall Audio", fileName = "WaterfallAudio")]
    public sealed class WaterfallAudioProfile : ScriptableObject
    {
        [SerializeField] private AudioClip loop;
        [Range(0f, 1f)] [SerializeField] private float volume = .65f;
        [Min(1f)] [SerializeField] private float minimumDistance = 8f;
        [Min(2f)] [SerializeField] private float maximumDistance = 150f;

        public AudioClip Loop => loop;
        public float Volume => Mathf.Clamp01(volume);
        public float MinimumDistance => Mathf.Max(1f, minimumDistance);
        public float MaximumDistance => Mathf.Max(MinimumDistance + 1f, maximumDistance);
    }
}
