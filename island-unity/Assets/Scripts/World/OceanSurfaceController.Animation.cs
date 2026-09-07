using System;
using UnityEngine;

public sealed partial class OceanSurfaceController
{
    private const double TwoPi = Math.PI * 2.0;
    private const float DirectionTransitionSeconds = 4f;
    private static readonly int[] WaveFromIds =
    {
        Shader.PropertyToID("_OceanWaveFrom0"), Shader.PropertyToID("_OceanWaveFrom1"),
        Shader.PropertyToID("_OceanWaveFrom2"), Shader.PropertyToID("_OceanWaveFrom3"),
    };
    private static readonly int[] WaveToIds =
    {
        Shader.PropertyToID("_OceanWaveTo0"), Shader.PropertyToID("_OceanWaveTo1"),
        Shader.PropertyToID("_OceanWaveTo2"), Shader.PropertyToID("_OceanWaveTo3"),
    };
    private static readonly int WaveTransitionId = Shader.PropertyToID("_OceanWaveTransition");
    private static readonly int OnshorePhaseId = Shader.PropertyToID("_OnshoreWavePhase");
    private static readonly int FoamTravelId = Shader.PropertyToID("_OceanFoamTravel");

    private struct WavePattern
    {
        internal Vector2 Direction;
        internal float Wavelength;
        internal double Phase;

        internal Vector4 ShaderValue => new Vector4(
            Direction.x, Direction.y, Wavelength, (float)Phase);

        internal void Advance(double travelMetres)
        {
            Phase = (Phase + travelMetres * TwoPi / Wavelength) % TwoPi;
        }
    }

    private readonly WavePattern[] outgoingWaves = new WavePattern[4];
    private readonly WavePattern[] incomingWaves = new WavePattern[4];
    private bool waveAnimationStarted;
    private bool changingWaveDirection;
    private float directionTransitionElapsed;
    private double onshorePhase;
    private double foamTravelX;
    private double foamTravelZ;
    private double fineFoamTravelX;
    private double fineFoamTravelZ;

    private float WaveTransition
    {
        get
        {
            if (!changingWaveDirection) return 0f;
            var progress = Mathf.Clamp01(directionTransitionElapsed / DirectionTransitionSeconds);
            return progress * progress * (3f - 2f * progress);
        }
    }

    private OceanWaveComponent WaveComponent(int index)
    {
        switch (index)
        {
            case 0: return waveSettings.Wave0;
            case 1: return waveSettings.Wave1;
            case 2: return waveSettings.Wave2;
            default: return waveSettings.Wave3;
        }
    }

    private WavePattern RequestedPattern(int index)
    {
        var wave = WaveComponent(index);
        return new WavePattern
        {
            // Each component is an independent world-space direction supplied
            // by the weather script; wind does not rotate the spectrum.
            Direction = wave.Direction.normalized,
            Wavelength = wave.WavelengthMetres,
        };
    }

    private void ResetWaveAnimation()
    {
        for (var index = 0; index < 4; index++)
        {
            outgoingWaves[index] = incomingWaves[index] = RequestedPattern(index);
        }
        changingWaveDirection = false;
        directionTransitionElapsed = 0f;
        onshorePhase = 0.0;
        foamTravelX = foamTravelZ = fineFoamTravelX = fineFoamTravelZ = 0.0;
        BindWaveAnimation();
    }

    private void LateUpdate()
    {
        AdvanceWaveAnimation(Time.deltaTime);
    }

    // Run once after weather updates, never once per camera or material update.
    private void AdvanceWaveAnimation(float deltaTime)
    {
        if (surfaceMaterial == null) return;
        WeatherValueValidation.RequireFinite(deltaTime, nameof(deltaTime));
        deltaTime = Mathf.Max(deltaTime, 0f);
        waveAnimationStarted = true;
        if (!changingWaveDirection)
        {
            var changed = false;
            for (var index = 0; index < 4; index++)
            {
                var requested = RequestedPattern(index);
                changed |= (requested.Direction - incomingWaves[index].Direction).sqrMagnitude > 1.0e-8f
                    || requested.Wavelength != incomingWaves[index].Wavelength;
            }
            if (changed)
            {
                for (var index = 0; index < 4; index++)
                {
                    outgoingWaves[index] = incomingWaves[index];
                    var requested = RequestedPattern(index);
                    requested.Phase = incomingWaves[index].Phase;
                    incomingWaves[index] = requested;
                }
                changingWaveDirection = true;
                directionTransitionElapsed = 0f;
            }
        }

        // Both patterns stay fixed throughout the fade. Continuously changing
        // requests are picked up when the next transition starts.
        var previousBlend = WaveTransition;
        directionTransitionElapsed += deltaTime;
        for (var index = 0; index < 4; index++)
        {
            var travel = (double)deltaTime * WaveComponent(index).SpeedMetresPerSecond * weatherWaveScale;
            outgoingWaves[index].Advance(travel);
            incomingWaves[index].Advance(travel);
        }
        onshorePhase = (onshorePhase + (double)deltaTime
            * waveSettings.OnshoreWaveSpeedMetresPerSecond * weatherWaveScale
            * TwoPi / waveSettings.OnshoreWaveWavelengthMetres) % TwoPi;

        var foamDirection = Vector2.Lerp(
            outgoingWaves[0].Direction, incomingWaves[0].Direction,
            (previousBlend + WaveTransition) * 0.5f);
        var foamDistance = (double)deltaTime * waveSettings.Wave0.SpeedMetresPerSecond * weatherWaveScale;
        foamTravelX += foamDirection.x * foamDistance;
        foamTravelZ += foamDirection.y * foamDistance;
        fineFoamTravelX -= foamDirection.x * foamDistance * waveSettings.WhitecapCounterflowSpeed;
        fineFoamTravelZ -= foamDirection.y * foamDistance * waveSettings.WhitecapCounterflowSpeed;

        if (changingWaveDirection && directionTransitionElapsed >= DirectionTransitionSeconds)
        {
            Array.Copy(incomingWaves, outgoingWaves, 4);
            changingWaveDirection = false;
            directionTransitionElapsed = 0f;
        }
        BindWaveAnimation();
    }

    private void BindWaveAnimation()
    {
        if (surfaceMaterial == null) return;
        for (var index = 0; index < 4; index++)
        {
            surfaceMaterial.SetVector(WaveFromIds[index], outgoingWaves[index].ShaderValue);
            surfaceMaterial.SetVector(WaveToIds[index], incomingWaves[index].ShaderValue);
        }
        surfaceMaterial.SetFloat(WaveTransitionId, WaveTransition);
        surfaceMaterial.SetFloat(OnshorePhaseId, (float)onshorePhase);
        surfaceMaterial.SetVector(FoamTravelId, new Vector4(
            (float)foamTravelX, (float)foamTravelZ, (float)fineFoamTravelX, (float)fineFoamTravelZ));
    }
}
