use std::{fmt, str::FromStr};

/// Selects the implementation used to build an island.
///
/// CPU generation is always available and remains the default. GPU generation
/// accelerates hydraulic erosion and rock settling while retaining the CPU
/// river and waterfall builder. It is available when the crate is built with
/// the `gpu-generation` feature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GenerationMethod {
    #[default]
    Cpu,
    Gpu,
}

impl GenerationMethod {
    pub const ALL: [Self; 2] = [Self::Cpu, Self::Gpu];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
        }
    }

    #[must_use]
    pub const fn is_available(self) -> bool {
        match self {
            Self::Cpu => true,
            Self::Gpu => cfg!(feature = "gpu-generation"),
        }
    }

    pub(crate) fn require_available(self) -> Result<(), String> {
        self.is_available()
            .then_some(())
            .ok_or_else(|| format!("{self} generation requires the gpu-generation Cargo feature"))
    }

    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Cpu => 0,
            Self::Gpu => 1,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Cpu),
            1 => Some(Self::Gpu),
            _ => None,
        }
    }
}

impl fmt::Display for GenerationMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for GenerationMethod {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "gpu" => Ok(Self::Gpu),
            _ => Err(format!(
                "unknown generation method {value:?}; expected cpu or gpu"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GenerationMethod;

    #[test]
    fn method_names_round_trip() {
        for method in GenerationMethod::ALL {
            assert_eq!(method.as_str().parse(), Ok(method));
            assert_eq!(GenerationMethod::from_tag(method.tag()), Some(method));
        }
    }

    #[cfg(not(feature = "gpu-generation"))]
    #[test]
    fn unavailable_gpu_message_has_one_authority() {
        assert_eq!(
            GenerationMethod::Gpu.require_available(),
            Err(String::from(
                "gpu generation requires the gpu-generation Cargo feature"
            ))
        );
        assert_eq!(GenerationMethod::Cpu.require_available(), Ok(()));
    }
}
