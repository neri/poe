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

pub struct Unknown<KnownType, RawType> {
    raw: RawType,
    _phantom: PhantomData<KnownType>,
}

impl<KnownType, RawType> Unknown<KnownType, RawType>
where
    RawType: From<KnownType>,
    KnownType: TryFrom<RawType, Error = RawType>,
{
    #[inline]
    pub fn unknown(value: RawType) -> Self {
        Self {
            raw: value,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn known(value: KnownType) -> Self {
        Self {
            raw: value.into(),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn into_raw(self) -> RawType {
        self.raw
    }

    #[inline]
    pub fn as_raw(&self) -> RawType
    where
        RawType: Copy,
    {
        self.into_raw()
    }

    #[inline]
    pub fn into_known_value(self) -> Result<KnownType, RawType> {
        KnownType::try_from(self.raw)
    }

    #[inline]
    pub fn known_value(&self) -> Result<KnownType, RawType>
    where
        RawType: Copy,
    {
        self.into_known_value()
    }

    #[inline]
    pub fn has_known_value(&self) -> bool
    where
        RawType: Copy,
    {
        matches!(self.known_value(), Ok(_))
    }
}

impl<KnownType, RawType: Clone> Clone for Unknown<KnownType, RawType> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            _phantom: self._phantom.clone(),
        }
    }
}

impl<KnownType, RawType: Copy + Clone> Copy for Unknown<KnownType, RawType> {}

impl<KnownType, RawType> core::fmt::Debug for Unknown<KnownType, RawType>
where
    RawType: From<KnownType> + core::fmt::Debug + Copy,
    KnownType: TryFrom<RawType, Error = RawType> + core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.known_value() {
            Ok(value) => write!(f, "{:?}", value),
            Err(raw) => write!(f, "Unknown({:?})", raw),
        }
    }
}

impl<KnownType: PartialEq, RawType: PartialEq> PartialEq for Unknown<KnownType, RawType> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<KnownType: PartialOrd, RawType: PartialOrd> PartialOrd for Unknown<KnownType, RawType> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.raw.partial_cmp(&other.raw)
    }
}

impl<KnownType: Eq, RawType: Eq> Eq for Unknown<KnownType, RawType> {}

impl<KnownType: Ord, RawType: Ord> Ord for Unknown<KnownType, RawType> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}
