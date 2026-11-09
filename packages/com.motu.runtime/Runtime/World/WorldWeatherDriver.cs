using UnityEngine;

namespace Motu.World
{
    /// <summary>
    /// Assign a component derived from this class to the world's Weather Driver slot.
    /// It runs on the main thread before the environment applies wind, waves and clouds.
    /// </summary>
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "WorldWeatherDriver")]
    public abstract class WorldWeatherDriver : MonoBehaviour
    {
        /// <summary>
        /// Edit the current runtime state in place. deltaTime uses unscaled seconds,
        /// matching the environment clock. Disabled drivers leave the last state active.
        /// </summary>
        public abstract void UpdateWeather(ref WorldWeatherState weather, float deltaTime);

        /// <summary>When true, this driver owns automatic squall scheduling.
        /// Disabling that driver pauses spawning while existing storms keep moving.</summary>
        public virtual bool ControlsSquallSpawning => false;

        /// <summary>Current weather's support for squalls, from zero to one.
        /// Controls their population and strength throughout their lifetime.</summary>
        public virtual float GetSquallActivity(WorldWeatherState weather) => 1f;

        /// <summary>Runs after weather is applied and storms move. Uses scaled
        /// game seconds, so pausing the game also pauses storm spawning.</summary>
        public virtual void UpdateSquallSpawning(WorldEnvironmentController environment, float deltaTime) { }
    }
}
