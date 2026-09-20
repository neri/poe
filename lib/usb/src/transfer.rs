#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferProgress {
    Known(usize),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataPid {
    Data0,
    Data1,
    Setup,
}

impl DataPid {
    pub const fn toggled(self) -> Self {
        match self {
            Self::Data0 => Self::Data1,
            Self::Data1 => Self::Data0,
            Self::Setup => Self::Data1,
        }
    }
}
