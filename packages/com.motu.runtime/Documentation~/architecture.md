# Runtime boundaries and ownership

- `IslandGenerationRequest`: typed snapshot of settings and native options.
- `IslandGenerationWorker`: bounded preparation queue; Rust work runs off the main thread.
- `IslandPreparationPipeline`: cache/native acquisition and validated managed exports.
- `IslandGenerator`: public main-thread generation, cancellation, queries and lifecycle facade.
- `IslandRuntimeInstaller`: main-thread Unity resource creation with a shared frame budget.
- `IslandRuntime`: owns native handle, meshes, materials, textures, streamers, noise leases,
  and the optional navigation lifetime. Disposal is idempotent.
- `SingleIsland`: optional Inspector setup using a host-owned camera/lighting environment.
- `IslandWorldManager`: discovery, residency, scheduling and focus for an archipelago.
- `WorldEnvironmentController`: explicitly selected owner of global Motu water/weather.
- `Motu.Navigation`: separate optional assembly/package using core prepared data.
- `Motu.Editor`: generic inspectors/authoring, with no runtime dependency on editor APIs.
- `Motu.Samples` and `Motu.Samples.Editor`: development project only.

C ABI exports own their buffers until the matching release function is called.
C# copies and validates data before release. Native handles must outlive streaming
queries. Requests/outputs use explicit normalized/native versus metre/world-space
conversion at interop; the generator is native Z-up and Unity Y-up.

No operation may free native data still in use by an asynchronous operation. The
worker queue bounds concurrent preparation, while the navigation queue separately
bounds bake memory. Cancellation does not interrupt arbitrary Rust instructions.
The release build uses panic=abort; ABI checks cannot recover a process-level abort.

Authored materials/textures are borrowed. Runtime meshes and material instances
are owned. Shared immutable noise uses leases; the final release destroys the
texture. No asset is written into the installed package at runtime.

Shader equations and generation algorithms are preserved during package extraction.
Global water state is a single-environment contract, not a per-island singleton
that may be reset by island teardown. Standalone terrain borrows host lighting
and owns no global weather state.
