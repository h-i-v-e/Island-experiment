# Gravity-linked orbits

The planet follows an analytic Kepler ellipse around the Sun. Its year and
velocity derive from its semimajor axis and the two bodies' effective masses:

```
mu = G * (primary_mass + secondary_mass)
period = 2*pi*sqrt(semimajor_axis^3 / mu)
```

The moon uses the same law for its circular orbit around the planet, including
its own mass. Neither body has an independent period that can contradict its
orbital distance. In **Solar system**, changing **Year** adjusts the planet's
orbit distance; changing that distance, Sun size or planet size recomputes the
year. In **Companion moon**, period and distance are likewise linked. Moon
controls apply live; Sun/planet draft changes use **Apply system**.

This is a game-scale gravity model: planet surface gravity remains 9.81 m/s²
(the same constant as the walking controller), and Sun surface gravity is
274 m/s². Their effective masses derive from these accelerations and radii.
The moon's density is 0.606 times the planet's. At the default 2000 m radius:

- Sun centre distance: about 2498 km; planet speed: 2287 m/s; year: 114.36 physical minutes.
- Moon centre distance: 6000 m; speed: 81.0 m/s; period: 7.76 physical minutes.

**Day/night cycle** still independently sets the duration of a 24-hour planet
day (30 real minutes by default). Changing it preserves orbital and spin phase.
Orbital motion uses physical seconds; rotation and atmospheric evolution use
the accelerated day clock. The overall simulation-speed multiplier and pause
apply to both. Weather and the rendered Sun sample the same pair of clocks.

The default Sun is close enough to match the overhead Moon's apparent disk
from sea level (about 10.04 degrees in diameter). This is a geometric match at
conjunction, rather than a disk that follows the camera: lunar parallax means
the Moon appears smaller towards the horizon and from orbit. Eccentricity also
varies the Sun's apparent size throughout the year.

**Solar system → Match Moon disk** recalculates the distance for the current
Sun/Moon sizes and lunar distance; then **Apply system** activates it. Matching
also reduces the Sun's effective luminosity to preserve existing daylight and
weather heating. Subsequent ordinary orbit edits retain inverse-square falloff.
The saved irradiance reference keeps this calibration separate from gravity.
Legacy preferences using the original untouched solar distance are upgraded on
load, preserving other settings and pending edits. Custom distances and their
legacy light calibration are retained; use the button to match them explicitly.

The moon remains tidally locked. Its orbit is limited to less than 45% of the
planet's periapsis Hill radius as a conservative stability check. This is an
analytic two-body approximation; mutual three-body perturbations are not
integrated.

CLI `--solar-year D` sets a physical period in 24-hour days and derives orbital
distance. `--companion-period M` now sets a period in physical minutes and
derives moon distance (previously this flag used planet days). Later explicit
distance/period flags take precedence. Body sizes are resolved before periods,
so argument order relative to `--radius` or `--solar-radius` does not matter.
Saved distances are preserved; obsolete independent period fields are ignored.

Sources: [NASA Kepler's laws](https://science.nasa.gov/solar-system/orbits-and-keplers-laws/),
[Sun fact sheet](https://nssdc.gsfc.nasa.gov/planetary/factsheet/sunfact.html),
[Moon fact sheet](https://nssdc.gsfc.nasa.gov/planetary/factsheet/moonfact.html).

## A physical, visitable Sun

The Sun is an opaque emissive sphere at heliocentric **(0, 0, 0)** with the
same radius used by the gravity and apparent-disk calculations. It has a
seamless procedural photosphere, granulation, small sunspots and subtle bloom.
The old sky-painted disk is disabled whenever the physical body is active;
manual lighting with `--no-solar` retains the sky disk as a fallback.

The GPU scene retains its convenient rotating planet-local frame. Double
precision transformations on `SolarState` convert heliocentric positions to
and from local metres; the Sun, Moon and free-flight camera use this shared
frame. Planet radius is still the only conversion to normalized render units.
Camera far clipping now covers the actual Sun and planet rather than stopping
at 50 planet radii. The visible Sun has ordinary depth, parallax and occlusion.

- **View → Sun** (`--view sun`) follows the Sun and allows zooming towards its surface.
- **View → Solar system** (`--view solar`) frames the star and orbit at their actual scale. An orbital guide and blue
  navigation marker identify the otherwise tiny planet; its terrain mesh keeps its real size.
- **View → System** keeps the existing nearby planet/Moon overview.
- **F** enters free flight from orbit or walking. **WASD** moves, **Q/E** moves
  down/up, and the mouse steers. The wheel adjusts speed; Shift gives a 10× boost.
- **Tab** releases the mouse and pauses input; click the world to capture it again.
  **F** or **Esc** returns to the previous inspection view.

Free flight uses physical heliocentric metres, so the accelerated day clock
cannot drag the camera around. A swept sphere check stops the camera 100 m
above the solar photosphere, including movement steps that would cross the
whole sphere. This is an inspection flight controller; spacecraft fuel,
orbital acceleration, heat damage and planetary landing physics are separate
future gameplay systems.
