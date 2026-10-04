//! `event_body!` (every body, Var/Multi prefix and container struct), `wire_enum!` (wire enums)
//! and `layout!` (offset assertions).

/// Declares a `#[repr(C)]` struct with its `zerocopy` derives, `FIELDS` table, `FieldType` impl,
/// `Body` impl (fixed/var/multi forms) and the compile-time assertions of README "实现说明":
/// size a multiple of 8, size ≤ 65 504, equal to the `KINDS` registration (when registered),
/// and `prefix + max ≤ 65 504` for Var prefixes.
///
/// ```text
/// event_body! {
///     /// doc
///     pub struct Name : fixed(EventKind::X, 1), FromBytes {      // or var(kind, ver, max) / multi(kind, ver) / plain
///         /// field doc
///         pub a: u64 => measure,                                  // measurement field
///         pub b: u8 = FieldTy::Enum(&codes::X),                   // explicit FieldTy
///         pub _pad: [u8; 7],
///     }
/// }
/// ```
/// The read trait is `FromBytes` (integer-only bodies) or `TryFromBytes` (bodies with enums).
#[macro_export]
macro_rules! event_body {
    (
        $(#[$m:meta])*
        pub struct $name:ident : fixed($kind:expr, $ver:expr), $read:ident {
            $($fields:tt)*
        }
    ) => {
        $crate::event_body!(@struct [$(#[$m])*] $name $read { $($fields)* });
        impl $crate::Body for $name {
            const KIND: $crate::EventKind = $kind;
            const SCHEMA_VERSION: u16 = $ver;
        }
        const _: () = assert!(
            $crate::kinds::registered_ok($kind, 0, size_of::<$name>(), 0, $ver),
            "body size or schema_version differs from its KINDS registration"
        );
    };
    (
        $(#[$m:meta])*
        pub struct $name:ident : var($kind:expr, $ver:expr, $max:expr), $read:ident {
            $($fields:tt)*
        }
    ) => {
        $crate::event_body!(@struct [$(#[$m])*] $name $read { $($fields)* });
        impl $crate::Body for $name {
            const KIND: $crate::EventKind = $kind;
            const SCHEMA_VERSION: u16 = $ver;
        }
        const _: () = assert!(
            size_of::<$name>() + ($max as usize) <= $crate::MAX_BODY,
            "Var prefix + max exceeds 65 504"
        );
        const _: () = assert!(
            $crate::kinds::registered_ok($kind, 1, size_of::<$name>(), $max as usize, $ver),
            "Var prefix size, max or schema_version differs from its KINDS registration"
        );
    };
    (
        $(#[$m:meta])*
        pub struct $name:ident : multi($kind:expr, $ver:expr), $read:ident {
            $($fields:tt)*
        }
    ) => {
        $crate::event_body!(@struct [$(#[$m])*] $name $read { $($fields)* });
        impl $crate::Body for $name {
            const KIND: $crate::EventKind = $kind;
            const SCHEMA_VERSION: u16 = $ver;
        }
        const _: () = assert!(
            $crate::kinds::registered_ok($kind, 2, size_of::<$name>(), 0, $ver),
            "Multi prefix size or schema_version differs from its KINDS registration"
        );
    };
    (
        $(#[$m:meta])*
        pub struct $name:ident : plain, $read:ident {
            $($fields:tt)*
        }
    ) => {
        $crate::event_body!(@struct [$(#[$m])*] $name $read { $($fields)* });
    };
    (@struct [$($m:tt)*] $name:ident $read:ident {
        $( $(#[doc = $doc:literal])* pub $f:ident : $t:ty $(= $fty:expr)? $(=> $meas:ident)? ),* $(,)?
    }) => {
        $($m)*
        #[repr(C)]
        #[derive(
            Copy, Clone, Eq, PartialEq, Debug,
            zerocopy::$read, zerocopy::IntoBytes, zerocopy::KnownLayout, zerocopy::Immutable
        )]
        pub struct $name { $( $(#[doc = $doc])* pub $f: $t ),* }
        impl $name {
            /// Field table (offsets from `offset_of!`).
            pub const FIELDS: &'static [$crate::schema::FieldDesc] = &[$(
                $crate::schema::FieldDesc {
                    name: $crate::schema::field_name(stringify!($f)),
                    offset: core::mem::offset_of!($name, $f) as u16,
                    ty: $crate::schema::field_ty(
                        $crate::schema::field_name(stringify!($f)),
                        $crate::event_body!(@ty $t $(, $fty)?),
                        size_of::<$t>(),
                    ),
                    measure: $crate::event_body!(@meas $($meas)?),
                    doc: concat!("" $(, $doc)*),
                }
            ),*];
        }
        impl $crate::schema::FieldType for $name {
            const TY: $crate::schema::FieldTy = $crate::schema::FieldTy::Nested(<$name>::FIELDS);
        }
        const _: () = assert!(size_of::<$name>() % 8 == 0, "size is not a multiple of 8");
        const _: () = assert!(size_of::<$name>() <= $crate::MAX_BODY, "size exceeds 65 504");
    };
    (@ty $t:ty) => { <$t as $crate::schema::FieldType>::TY };
    (@ty $t:ty, $e:expr) => { $e };
    (@meas) => { false };
    (@meas measure) => { true };
}

/// Wire enum: `#[repr(u8|u16)]`, zerocopy `TryFromBytes`, `DESC` code table, `TryFrom<repr>`
/// (`0`/unknown → `DecodeErr::InvalidEnum`) and the `FieldType` impl.
macro_rules! wire_enum {
    ($(#[$m:meta])* $name:ident : $repr:ident = $desc:literal { $($v:ident = $n:literal),* $(,)? }) => {
        $(#[$m])*
        #[repr($repr)]
        #[derive(
            Copy, Clone, Eq, PartialEq, Hash, Debug,
            zerocopy::TryFromBytes, zerocopy::IntoBytes, zerocopy::KnownLayout, zerocopy::Immutable
        )]
        pub enum $name { $($v = $n),* }
        impl $name {
            /// Code table (`gen-schema`).
            pub const DESC: $crate::schema::EnumDesc = $crate::schema::EnumDesc {
                name: $desc,
                width: size_of::<$repr>() as u8,
                items: &[$($crate::schema::EnumItem { name: stringify!($v), value: $n }),*],
            };
            /// Every variant in code order.
            pub const ALL: &'static [$name] = &[$($name::$v),*];
            /// Decodes a wire value; illegal values → `None`.
            #[inline]
            pub const fn from_repr(v: $repr) -> Option<$name> {
                match v { $($n => Some($name::$v),)* _ => None }
            }
        }
        impl TryFrom<$repr> for $name {
            type Error = $crate::DecodeErr;
            #[inline]
            fn try_from(v: $repr) -> Result<Self, $crate::DecodeErr> {
                Self::from_repr(v).ok_or($crate::DecodeErr::InvalidEnum)
            }
        }
        impl $crate::schema::FieldType for $name {
            const TY: $crate::schema::FieldTy = $crate::schema::FieldTy::Enum(&<$name>::DESC);
        }
        const _: () = assert!(size_of::<$name>() == size_of::<$repr>());
    };
}
pub(crate) use wire_enum;

/// Compile-time size, alignment and offset assertions (README layout comments).
macro_rules! layout {
    ($t:ty, $size:expr, $align:expr; $($f:ident @ $off:expr),* $(,)?) => {
        const _: () = {
            assert!(size_of::<$t>() == $size);
            assert!(align_of::<$t>() == $align);
            $(assert!(core::mem::offset_of!($t, $f) == $off);)*
        };
    };
}
pub(crate) use layout;
