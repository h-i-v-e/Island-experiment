use std::{fmt, str::FromStr};

/// Selects the implementation used to build an island.
///
/// CPU generation is always available and remains the default. GPU generation
/// is available when the crate is built with the `gpu-generation` feature.
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
        }
    }
}
