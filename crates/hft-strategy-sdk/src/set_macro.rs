//! `strategy_set!` and `reset_preview_of` (README "装配:StrategySet 与 strategy_set!").

use crate::group::{GroupSpecExt, GroupTable};
use crate::set::ResetPreview;
use crate::strategy::{Model, ParamSet, Strategy};

/// `StrategySet::reset_preview` of one kind (README "参数变化时的 group 重置" 3): over the new
/// table aligned by GroupId, a group restarts empty when its GroupId is new, its fingerprint
/// changed, or `structural_changed`; `no_open_ms` = max of `cold_start_ms(new, gix)` (`WARMUP`:
/// `min(cold_start_ms, path_settle_ms)`), saturating to `u32::MAX`.
pub fn reset_preview_of<S: Strategy>(
    old: &S::Params,
    old_g: &GroupTable,
    new: &S::Params,
    new_g: &GroupTable,
) -> ResetPreview {
    let mut reset_groups = 0u32;
    let mut no_open = 0u64;
    for (gix, spec) in new_g.iter() {
        let reset = match old_g.gix_of(spec.id) {
            None => true,
            Some(og) => {
                old_g.get(og).fingerprint() != spec.fingerprint() || new.structural_changed(old, og, gix)
            }
        };
        if !reset {
            continue;
        }
        reset_groups = reset_groups.saturating_add(1);
        let cold = <S::Model as Model>::cold_start_ms(new, gix);
        let est = if <S::Model as Model>::WARMUP {
            cold.min(<S::Model as Model>::path_settle_ms(new))
        } else {
            cold
        };
        no_open = no_open.max(est);
    }
    ResetPreview {
        reset_groups,
        no_open_ms: u32::try_from(no_open).unwrap_or(u32::MAX),
    }
}

#[cfg(feature = "schema-json")]
#[doc(hidden)]
#[macro_export]
macro_rules! __sdk_if_schema_json {
    ($($t:tt)*) => { $($t)* };
}

#[cfg(not(feature = "schema-json"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __sdk_if_schema_json {
    ($($t:tt)*) => {};
}

/// Assembles strategy kinds into one `StrategySet` (README "装配:StrategySet 与 strategy_set!").
///
/// ```ignore
/// hft_strategy_sdk::strategy_set! {
///     pub struct Strategies {
///         foo::Foo,
///         #[cfg(feature = "bar")] bar::Bar,
///     }
/// }
/// ```
///
/// Generates the unit struct, a private module `__strategy_set` with the instance, model and
/// parameter enums (one variant per row, named after the path's last segment) and
/// `impl StrategySet`: every method is one `match` with an inlined call — static dispatch, no
/// trait objects, no boxing. Compile-time checks (`check::validate_set`): every descriptor valid
/// and `kind_id`, `name`, `schema.id` unique. Rows may carry `#[cfg(...)]`.
#[macro_export]
macro_rules! strategy_set {
    ($(#[$sm:meta])* $vis:vis struct $name:ident { $($rows:tt)* }) => {
        $crate::strategy_set!(@rows [$(#[$sm])*] [$vis] $name [] $($rows)*);
    };
    (@rows $sm:tt $vis:tt $name:ident [$($done:tt)*]) => {
        $crate::strategy_set!(@gen $sm $vis $name $($done)*);
    };
    (@rows $sm:tt $vis:tt $name:ident [$($done:tt)*]
        $(#[$m:meta])* $seg:ident $(:: $more:ident)* $(, $($rest:tt)*)?) => {
        $crate::strategy_set!(@last $sm $vis $name [$($done)*] [$(#[$m])*] [$seg $(:: $more)*]
            [$seg $($more)*] [$($($rest)*)?]);
    };
    (@last $sm:tt $vis:tt $name:ident [$($done:tt)*] $attrs:tt $path:tt [$last:ident] [$($rest:tt)*]) => {
        $crate::strategy_set!(@rows $sm $vis $name [$($done)* ($attrs $path $last)] $($rest)*);
    };
    (@last $sm:tt $vis:tt $name:ident $done:tt $attrs:tt $path:tt [$x:ident $($xs:ident)+] $rest:tt) => {
        $crate::strategy_set!(@last $sm $vis $name $done $attrs $path [$($xs)+] $rest);
    };
    (@gen [$($sm:tt)*] [$vis:vis] $name:ident $(([$($attr:tt)*] [$($path:tt)*] $var:ident))*) => {
        $($sm)*
        #[derive(Clone, Copy, Debug)]
        $vis struct $name;

        #[allow(missing_debug_implementations, unused_imports, clippy::large_enum_variant)]
        mod __strategy_set {
            use super::*;
            /// One instance of a registered strategy kind.
            pub enum Inst {
                $($($attr)* $var($($path)*),)*
            }
            /// The Model of one instance.
            pub enum ModelInst {
                $($($attr)* $var(<$($path)* as $crate::Strategy>::Model),)*
            }
            /// Decoded parameters of one kind.
            pub enum Params {
                $($($attr)* $var(<$($path)* as $crate::Strategy>::Params),)*
            }
            impl Inst {
                /// `kind_id` of the instance's strategy.
                pub fn kind_id(&self) -> u16 {
                    match self {
                        $($($attr)* Inst::$var(_) => <$($path)* as $crate::Strategy>::DESC.kind_id,)*
                    }
                }
            }
            impl Params {
                /// `kind_id` the parameters belong to.
                pub fn kind_id(&self) -> u16 {
                    match self {
                        $($($attr)* Params::$var(_) => <$($path)* as $crate::Strategy>::DESC.kind_id,)*
                    }
                }
            }
        }

        impl $name {
            /// Descriptors of the registered kinds, in list order.
            pub const KINDS: &'static [$crate::StrategyDesc] = &[
                $($($attr)* <$($path)* as $crate::Strategy>::DESC,)*
            ];
        }

        const _: () = $crate::check::validate_set($name::KINDS);

        impl $crate::StrategySet for $name {
            type Inst = __strategy_set::Inst;
            type ModelInst = __strategy_set::ModelInst;
            type Params = __strategy_set::Params;

            fn kinds() -> &'static [$crate::StrategyDesc] {
                $name::KINDS
            }
            fn decode_params(
                kind_id: u16,
                body: &[u8],
                schema_ver: u16,
                g: &$crate::GroupTable,
            ) -> ::core::result::Result<Self::Params, $crate::ParamError> {
                match kind_id {
                    $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                        <<$($path)* as $crate::Strategy>::Params as $crate::ParamSet>::decode(body, schema_ver, g)
                            .map(__strategy_set::Params::$var)
                    })*
                    _ => ::core::result::Result::Err($crate::ParamError::SchemaMismatch {
                        want: 0,
                        got: u32::from(kind_id),
                    }),
                }
            }
            fn validate_params(
                p: &Self::Params,
                g: &$crate::GroupTable,
                rd: &$crate::RefView<'_>,
            ) -> ::core::result::Result<(), $crate::ParamError> {
                match p {
                    $($($attr)* __strategy_set::Params::$var(x) => $crate::ParamSet::validate(x, g, rd),)*
                }
            }
            fn create(
                i: $crate::StrategyInstance,
                p: &Self::Params,
                g: &$crate::GroupTable,
                rd: &$crate::RefView<'_>,
            ) -> ::core::result::Result<Self::Inst, $crate::ParamError> {
                match p {
                    $($($attr)* __strategy_set::Params::$var(x) => {
                        <$($path)* as $crate::Strategy>::new(i, x, g, rd).map(__strategy_set::Inst::$var)
                    })*
                }
            }
            fn create_model(
                i: $crate::StrategyInstance,
                p: &Self::Params,
                g: &$crate::GroupTable,
                rd: &$crate::RefView<'_>,
            ) -> Self::ModelInst {
                match p {
                    $($($attr)* __strategy_set::Params::$var(x) => __strategy_set::ModelInst::$var(
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::new(i, x, g, rd),
                    ),)*
                }
            }
            #[inline]
            fn on_event<V: $crate::Views>(
                s: &mut Self::Inst,
                ev: &$crate::EventRef<'_>,
                v: &V,
                ctx: &mut $crate::Ctx<'_>,
            ) {
                match s {
                    $($($attr)* __strategy_set::Inst::$var(x) => {
                        <$($path)* as $crate::Strategy>::on_event(x, ev, v, ctx)
                    })*
                }
            }
            fn on_params(
                s: &mut Self::Inst,
                p: &Self::Params,
                g: &$crate::GroupTable,
                rd: &$crate::RefView<'_>,
            ) -> ::core::result::Result<(), $crate::ParamError> {
                let (want, got) = (s.kind_id(), p.kind_id());
                match (s, p) {
                    $($($attr)* (__strategy_set::Inst::$var(x), __strategy_set::Params::$var(q)) => {
                        <$($path)* as $crate::Strategy>::on_params(x, q, g, rd)
                    })*
                    #[allow(unreachable_patterns)]
                    _ => ::core::result::Result::Err($crate::ParamError::SchemaMismatch {
                        want: u32::from(want),
                        got: u32::from(got),
                    }),
                }
            }
            fn subscriptions(s: &Self::Inst, g: &$crate::GroupTable, out: &mut $crate::SubsBuilder<'_>) {
                match s {
                    $($($attr)* __strategy_set::Inst::$var(x) => {
                        <$($path)* as $crate::Strategy>::subscriptions(x, g, out)
                    })*
                }
            }
            fn snapshot(s: &Self::Inst, w: &mut $crate::SnapshotWriter) {
                match s {
                    $($($attr)* __strategy_set::Inst::$var(x) => <$($path)* as $crate::Strategy>::snapshot(x, w),)*
                }
            }
            fn restore(
                s: &mut Self::Inst,
                r: &mut $crate::SnapshotReader<'_>,
            ) -> ::core::result::Result<(), $crate::RestoreError> {
                match s {
                    $($($attr)* __strategy_set::Inst::$var(x) => <$($path)* as $crate::Strategy>::restore(x, r),)*
                }
            }
            fn model_mem_bytes(p: &Self::Params, n_groups: u16) -> u64 {
                match p {
                    $($($attr)* __strategy_set::Params::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::mem_bytes(x, n_groups)
                    })*
                }
            }
            fn model_snapshot_bytes(p: &Self::Params, n_groups: u16) -> u64 {
                match p {
                    $($($attr)* __strategy_set::Params::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::snapshot_bytes(x, n_groups)
                    })*
                }
            }
            fn has_model(kind_id: u16) -> bool {
                match kind_id {
                    $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                        core::any::TypeId::of::<<$($path)* as $crate::Strategy>::Model>()
                            != core::any::TypeId::of::<$crate::NoModel<<$($path)* as $crate::Strategy>::Params>>()
                    })*
                    _ => false,
                }
            }
            fn model_layout(kind_id: u16) -> &'static $crate::ModelLayoutDesc {
                match kind_id {
                    $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::LAYOUT
                    })*
                    _ => &$crate::ModelLayoutDesc::EMPTY,
                }
            }
            fn model_layout_version(kind_id: u16) -> u32 {
                match kind_id {
                    $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::LAYOUT_VERSION
                    })*
                    _ => 0,
                }
            }
            fn model_on_event<V: $crate::ModelViews>(
                m: &mut Self::ModelInst,
                ev: &$crate::EventRef<'_>,
                v: &V,
                out: &mut $crate::AdviceOut<'_>,
            ) {
                match m {
                    $($($attr)* __strategy_set::ModelInst::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::on_event(x, ev, v, out)
                    })*
                }
            }
            fn model_on_params(
                m: &mut Self::ModelInst,
                p: &Self::Params,
                g: &$crate::GroupTable,
                rd: &$crate::RefView<'_>,
            ) -> $crate::ModelParamEffect {
                match (m, p) {
                    $($($attr)* (__strategy_set::ModelInst::$var(x), __strategy_set::Params::$var(q)) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::on_params(x, q, g, rd)
                    })*
                    #[allow(unreachable_patterns)]
                    _ => $crate::ModelParamEffect::None,
                }
            }
            fn model_snapshot_chunk(
                m: &Self::ModelInst,
                from_gix: u16,
                w: &mut $crate::SnapshotWriter,
                max_bytes: u32,
            ) -> $crate::SnapshotProgress {
                match m {
                    $($($attr)* __strategy_set::ModelInst::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::snapshot_chunk(x, from_gix, w, max_bytes)
                    })*
                }
            }
            fn model_restore(
                m: &mut Self::ModelInst,
                r: &mut $crate::SnapshotReader<'_>,
            ) -> ::core::result::Result<(), $crate::RestoreError> {
                match m {
                    $($($attr)* __strategy_set::ModelInst::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::restore(x, r)
                    })*
                }
            }
            fn model_path_version(kind_id: u16) -> u32 {
                match kind_id {
                    $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::PATH_VERSION
                    })*
                    _ => 0,
                }
            }
            fn model_snapshot_path(m: &Self::ModelInst, w: &mut $crate::SnapshotWriter) {
                match m {
                    $($($attr)* __strategy_set::ModelInst::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::snapshot_path(x, w)
                    })*
                }
            }
            fn model_restore_path(
                m: &mut Self::ModelInst,
                r: &mut $crate::SnapshotReader<'_>,
            ) -> ::core::result::Result<(), $crate::RestoreError> {
                match m {
                    $($($attr)* __strategy_set::ModelInst::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::restore_path(x, r)
                    })*
                }
            }
            fn model_path_settle_ms(p: &Self::Params) -> u64 {
                match p {
                    $($($attr)* __strategy_set::Params::$var(x) => {
                        <<$($path)* as $crate::Strategy>::Model as $crate::Model>::path_settle_ms(x)
                    })*
                }
            }
            fn reset_preview(
                old: &Self::Params,
                old_g: &$crate::GroupTable,
                new: &Self::Params,
                new_g: &$crate::GroupTable,
            ) -> $crate::ResetPreview {
                match (old, new) {
                    $($($attr)* (__strategy_set::Params::$var(a), __strategy_set::Params::$var(b)) => {
                        $crate::set::reset_preview_of::<$($path)*>(a, old_g, b, new_g)
                    })*
                    #[allow(unreachable_patterns)]
                    _ => $crate::ResetPreview::default(),
                }
            }
            $crate::__sdk_if_schema_json! {
                fn schema_json(kind_id: u16) -> ::core::option::Option<::std::string::String> {
                    match kind_id {
                        $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                            ::core::option::Option::Some($crate::params::schema_json(
                                &<$($path)* as $crate::Strategy>::DESC,
                                <<$($path)* as $crate::Strategy>::Model as $crate::Model>::LAYOUT,
                                <$($path)* as $crate::Strategy>::DESC.schema.version,
                            ))
                        })*
                        _ => ::core::option::Option::None,
                    }
                }
            }
            fn cmds(kind_id: u16) -> &'static [$crate::CmdDesc] {
                match kind_id {
                    $($($attr)* k if k == <$($path)* as $crate::Strategy>::DESC.kind_id => {
                        <$($path)* as $crate::Strategy>::DESC.cmds
                    })*
                    _ => &[],
                }
            }
        }
    };
}
