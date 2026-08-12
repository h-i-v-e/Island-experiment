#[cfg(feature = "profiling")]
use std::time::Instant;

pub(crate) struct StageTimer {
    #[cfg(feature = "profiling")]
    name: &'static str,
    #[cfg(feature = "profiling")]
    started: Instant,
}

impl StageTimer {
    #[inline]
    pub(crate) fn new(name: &'static str) -> Self {
        #[cfg(not(feature = "profiling"))]
        let _ = name;
        Self {
            #[cfg(feature = "profiling")]
            name,
            #[cfg(feature = "profiling")]
            started: Instant::now(),
        }
    }
}

#[cfg(feature = "profiling")]
impl Drop for StageTimer {
    fn drop(&mut self) {
        eprintln!(
            "profile,{},{:.3}",
            self.name,
            self.started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
