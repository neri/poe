use core::marker::PhantomData;

#[macro_export]
macro_rules! unknown_enum {
    {
        $(#[$attr1:meta])*
        $vis1:vis enum $ident:ident ( $repr:ident ) {
            $(
                $(#[$attr2:meta])*
                $variant:ident = $value:expr,
            )*
        }
    } => {
        $(#[$attr1])*
        #[repr($repr)]
        $vis1 enum $ident {
            $(
                $(#[$attr2])*
                $variant = $value,
            )*
        }

        impl From<$ident> for $repr {
            #[inline]
            fn from(value: $ident) -> Self {
                value as $repr
            }
        }

        impl TryFrom<$repr> for $ident {
            type Error = $repr;

            #[inline]
            fn try_from(value: $repr) -> Result<$ident, Self::Error> {
                match value {
                    $(
                        $value => Ok($ident::$variant),
                    )*
                    _ => Err(value),
                }
            }
        }
    };
}

pub struct Unknown<KnownType, RawType: Copy> {
    raw: RawType,
    _phantom: PhantomData<(RawType, KnownType)>,
}

impl<KnownType, RawType: Copy> Unknown<KnownType, RawType> {
    #[inline]
    pub const fn unknown(value: RawType) -> Self {
        Self {
            raw: value,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub const fn into_raw(self) -> RawType {
        self.raw
    }

    #[inline]
    pub const fn as_raw(&self) -> RawType {
        self.into_raw()
    }
}

impl<KnownType, RawType: Copy> Unknown<KnownType, RawType>
where
    RawType: From<KnownType>,
{
    #[inline]
    pub fn known(value: KnownType) -> Self {
        Self {
            raw: value.into(),
            _phantom: PhantomData,
        }
    }
}

impl<KnownType, RawType: Copy> Unknown<KnownType, RawType>
where
    KnownType: TryFrom<RawType, Error = RawType>,
{
    #[inline]
    pub fn into_known_value(self) -> Result<KnownType, RawType> {
        KnownType::try_from(self.raw)
    }

    #[inline]
    pub fn known_value(&self) -> Result<KnownType, RawType> {
        self.into_known_value()
    }

    #[inline]
    pub fn has_known_value(&self) -> bool {
        matches!(self.known_value(), Ok(_))
    }
}

impl<KnownType, RawType: Copy + Clone> Clone for Unknown<KnownType, RawType> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<KnownType, RawType: Copy + Clone> Copy for Unknown<KnownType, RawType> {}

impl<KnownType, RawType: Copy> core::fmt::Debug for Unknown<KnownType, RawType>
where
    RawType: core::fmt::Debug,
    KnownType: TryFrom<RawType, Error = RawType> + core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.known_value() {
            Ok(value) => write!(f, "{:?}", value),
            Err(raw) => write!(f, "Unknown({:?})", raw),
        }
    }
}

impl<KnownType: PartialEq, RawType: PartialEq + Copy> PartialEq for Unknown<KnownType, RawType> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<KnownType: PartialOrd, RawType: PartialOrd + Copy> PartialOrd for Unknown<KnownType, RawType> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.raw.partial_cmp(&other.raw)
    }
}

impl<KnownType: Eq, RawType: Eq + Copy> Eq for Unknown<KnownType, RawType> {}

impl<KnownType: Ord, RawType: Ord + Copy> Ord for Unknown<KnownType, RawType> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl<KnownType, RawType: Copy> From<KnownType> for Unknown<KnownType, RawType>
where
    RawType: From<KnownType>,
{
    #[inline]
    fn from(value: KnownType) -> Self {
        Self::known(value)
    }
}
