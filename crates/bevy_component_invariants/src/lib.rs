//! Mutually exclusive components — open enums spread across components.
//!
//! An *axis* is a set of components of which an entity carries at most one:
//! inserting a variant retracts whichever sibling was there before. The set is
//! **open**, so a downstream crate can add a variant to an axis it did not
//! define, and the crate that defined the axis never has to learn about it. That
//! is the difference from an enum component, where adding a variant means editing
//! the enum.
//!
//! An item's state is one axis; a weapon's fire mode, a character's stance and a
//! tool's durability tier are the same shape.
//!
//! A variant is declared entirely at its definition site:
//!
//! ```
//! use bevy_component_invariants::{AxisKey, StateAxis, variant_of};
//! use bevy_ecs::prelude::*;
//!
//! pub struct FireMode;
//! impl StateAxis for FireMode {
//!     const KEY: AxisKey = AxisKey("weapon::fire_mode");
//! }
//!
//! #[variant_of(FireMode)]
//! #[derive(Component, Clone, Copy)]
//! pub struct Burst;
//!
//! #[variant_of(FireMode)]
//! #[derive(Component, Clone, Copy)]
//! pub struct Single;
//! ```
//!
//! Add [`AxisPlugin`] once, and inserting `Burst` on an entity that has `Single`
//! removes the `Single`. Exclusivity rides on the component's own insert hook, so
//! a downstream crate cannot fail to uphold it — it was never that crate's
//! responsibility.
//!
//! There is no registration call. The attribute submits the variant to a
//! link-time collection and [`AxisPlugin`] drains that collection, so every
//! variant in the binary is in [`AxisRegistry`] before anything is ever
//! inserted. This is the mechanism `bevy_reflect` uses for `#[derive(Reflect)]`,
//! down to the same `inventory` dependency.
//!
//! Two cases fall back to registering on first insert instead, which is enough
//! to exclude correctly but not to enumerate:
//!
//! - **Generic variants**, which have no set of instantiations to collect — the
//!   same limitation `#[derive(Reflect)]` has.
//! - **Platforms `inventory` does not support**, where the collection is empty.
//!   On wasm the collection also depends on `__wasm_call_ctors` having run; Bevy
//!   calls it from `App::default()` under its default `reflect_auto_register`
//!   feature, so [`AxisPlugin`] — built later — sees a populated collection. Do
//!   not call it again from here: independent guards do not coordinate, and
//!   running it twice would submit everything twice.
//!
//! Exclusion never depends on which path a variant took, so the fallback is a
//! safety net rather than a mode. The hook warns when it uses it, so a variant
//! missing from the enumeration is audible rather than silent.

// So the absolute paths `#[variant_of]` expands to resolve inside this crate too.
extern crate self as bevy_component_invariants;

use std::collections::HashMap;
use std::fmt;

use bevy_app::prelude::*;
use bevy_ecs::component::ComponentId;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::world::DeferredWorld;
use bevy_log::prelude::*;

pub use bevy_component_invariants_macro::variant_of;

/// Installs the registry the exclusion hooks read, and fills it with every
/// variant linked into the binary. Without it the hooks warn and do nothing,
/// rather than panicking mid-insert.
pub struct AxisPlugin;

impl Plugin for AxisPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AxisRegistry>();
        // Before any entity exists, so `variants` and `axes` describe what the
        // binary can do rather than what it happens to have done.
        for entry in __private::inventory::iter::<VariantRegistration> {
            let id = (entry.register)(app.world_mut());
            app.world_mut()
                .resource_mut::<AxisRegistry>()
                .register(entry.key, id);
        }
    }
}

/// An axis's name — its own string namespace, so it can never be confused with
/// an `ItemKey` or a variant name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AxisKey(pub &'static str);

impl AxisKey {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for AxisKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A set of mutually exclusive components. The type is the axis's identity —
/// [`KEY`](StateAxis::KEY) is only its name, for display and serialization, so a
/// typo in the string cannot silently split one axis into two.
pub trait StateAxis: 'static {
    const KEY: AxisKey;
}

/// A variant's full name: its axis plus its own, printing as
/// `core::item_state::on_ground`. Carrying the axis means two axes can each
/// have an `on_ground` without colliding as map keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VariantKey {
    axis: AxisKey,
    name: &'static str,
}

impl VariantKey {
    pub const fn new(axis: AxisKey, name: &'static str) -> Self {
        Self { axis, name }
    }

    pub const fn axis(self) -> AxisKey {
        self.axis
    }

    pub const fn name(self) -> &'static str {
        self.name
    }
}

impl fmt::Display for VariantKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.axis.0, self.name)
    }
}

/// One member of an axis. Implemented only by
/// [`#[variant_of]`](variant_of), which is what ties membership to the hook
/// that enforces it — the two cannot drift apart.
pub trait VariantOf: Component + __private::Sealed {
    type Axis: StateAxis;
    const KEY: VariantKey;
}

/// One variant's entry in the link-time collection: its name, plus the one thing
/// that cannot be known statically. A `ComponentId` belongs to a `World`, so the
/// entry carries a way to obtain one rather than an id.
#[doc(hidden)]
pub struct VariantRegistration {
    pub key: VariantKey,
    pub register: fn(&mut World) -> ComponentId,
}

__private::inventory::collect!(VariantRegistration);

/// What [`#[variant_of]`](variant_of) puts in a [`VariantRegistration`]. A path
/// to a monomorphised function coerces to a `fn` pointer in the const context
/// `inventory::submit!` requires, where a closure is less reliable.
#[doc(hidden)]
pub fn register_component_of<V: VariantOf>(world: &mut World) -> ComponentId {
    world.register_component::<V>()
}

#[doc(hidden)]
pub mod __private {
    /// Only `#[variant_of]` emits this impl. Writing one by hand is not a
    /// mistake anyone makes by accident.
    pub trait Sealed {}

    /// So a crate using `#[variant_of]` needs no `inventory` dependency of its
    /// own — the attribute stays the whole declaration.
    pub use ::inventory;
}

/// Which components belong to which axis.
///
/// [`AxisPlugin`] fills this at build time from the link-time collection, so it
/// lists every non-generic variant in the binary before any entity exists — what
/// is *possible*, not merely what has *happened*. The two fallback cases in the
/// module docs (generic variants, and platforms without `inventory` support)
/// register on first insert instead and are absent until then.
#[derive(Resource, Default)]
pub struct AxisRegistry(HashMap<AxisKey, Vec<(ComponentId, VariantKey)>>);

impl AxisRegistry {
    /// Records a variant under its axis, keeping that axis's members ordered by
    /// variant name. Returns whether the variant was new.
    ///
    /// Sorted rather than in arrival order because arrival order is link order,
    /// which is arbitrary and free to change between builds — no use to a state
    /// listing or to anything keyed on position.
    fn register(&mut self, key: VariantKey, id: ComponentId) -> bool {
        let members = self.0.entry(key.axis()).or_default();
        if members.iter().any(|&(known, _)| known == id) {
            return false;
        }
        let at = members.partition_point(|&(_, known)| known.name() < key.name());
        members.insert(at, (id, key));
        true
    }

    fn members(&self, axis: AxisKey) -> &[(ComponentId, VariantKey)] {
        self.0.get(&axis).map_or(&[], Vec::as_slice)
    }

    /// Every axis with at least one known variant.
    pub fn axes(&self) -> impl Iterator<Item = AxisKey> + '_ {
        self.0.keys().copied()
    }

    /// Every variant known on `axis`, ordered by name.
    pub fn variants(&self, axis: AxisKey) -> impl Iterator<Item = VariantKey> + '_ {
        self.members(axis).iter().map(|&(_, key)| key)
    }

    /// The variant on `axis` called `name`, for code holding a string rather
    /// than a type — a saved game naming the state it wants back, or a
    /// compatibility check asking whether anything provides a given state.
    pub fn variant_by_name(&self, axis: AxisKey, name: &str) -> Option<VariantKey> {
        self.variants(axis).find(|key| key.name() == name)
    }

    /// The component behind a variant name.
    pub fn component_id_of(&self, key: VariantKey) -> Option<ComponentId> {
        self.members(key.axis())
            .iter()
            .find(|&&(_, known)| known == key)
            .map(|&(id, _)| id)
    }

    /// The variant of `A` the entity currently carries.
    ///
    /// Read this from a queued command rather than straight from an observer
    /// body. Exclusion is a deferred command (see [`enforce_axis`]), so during
    /// the observers of an insert the retracted sibling is still attached and
    /// this can report it instead of the incoming one.
    pub fn variant_on<A: StateAxis>(&self, entity: EntityRef) -> Option<VariantKey> {
        self.variant_on_key(entity, A::KEY)
    }

    /// [`variant_on`](Self::variant_on) for code that has an axis by name
    /// rather than by type — deferred work carrying a [`VariantKey`] it needs
    /// to re-check.
    pub fn variant_on_key(&self, entity: EntityRef, axis: AxisKey) -> Option<VariantKey> {
        self.members(axis)
            .iter()
            .find(|&&(id, _)| entity.contains_id(id))
            .map(|&(_, key)| key)
    }

    /// How many variants of `A` the entity carries. The hook keeps this at one
    /// or zero; anything else is a broken invariant worth reporting.
    pub fn count_on<A: StateAxis>(&self, entity: EntityRef) -> usize {
        self.members(A::KEY)
            .iter()
            .filter(|&&(id, _)| entity.contains_id(id))
            .count()
    }
}

/// Declaring that an axis is *total* over some population of entities.
pub trait AxisAppExt {
    /// Every entity matching `F` carries exactly one variant of `A` — never two,
    /// and never none.
    ///
    /// Exclusivity is the axis's own job and needs no declaring. Totality is a
    /// rule about a particular population, which the axis machinery cannot
    /// infer, so it is stated here and checked in debug builds. Reaching zero
    /// takes removing a marker directly, which the contract forbids.
    fn require_total_axis<A: StateAxis, F: QueryFilter + 'static>(&mut self) -> &mut Self;
}

impl AxisAppExt for App {
    fn require_total_axis<A: StateAxis, F: QueryFilter + 'static>(&mut self) -> &mut Self {
        if !self.is_plugin_added::<AxisPlugin>() {
            self.add_plugins(AxisPlugin);
        }
        #[cfg(debug_assertions)]
        self.add_systems(Last, check_axis_totality::<A, F>);
        self
    }
}

#[cfg(debug_assertions)]
fn check_axis_totality<A: StateAxis, F: QueryFilter + 'static>(
    entities: Query<EntityRef, F>,
    axes: Res<AxisRegistry>,
) {
    for entity in &entities {
        if let Some(violation) = axis_violation(axes.count_on::<A>(entity)) {
            error!(
                "{} invariant broken on {}: {violation}",
                A::KEY,
                entity.id()
            );
        }
    }
}

#[cfg(any(debug_assertions, test))]
fn axis_violation(marker_count: usize) -> Option<&'static str> {
    match marker_count {
        0 => Some("no variant present"),
        1 => None,
        _ => Some("more than one variant present"),
    }
}

/// The insert hook every variant carries: make sure it is known, then clear the
/// siblings.
///
/// Being a hook rather than an observer is what makes the ordering structural.
/// Hooks run before observers on the same insert, and the removal below is
/// queued from here, so it lands ahead of anything an observer queues. Note that
/// it lands *after* those observers run — see [`AxisRegistry::variant_on`].
pub fn enforce_axis<V: VariantOf>(mut world: DeferredWorld, context: HookContext) {
    let (entity, own) = (context.entity, context.component_id);

    let stale: Vec<ComponentId> = {
        let Some(mut registry) = world.get_resource_mut::<AxisRegistry>() else {
            warn_once!("`AxisPlugin` is missing; `{}` excludes nothing", V::KEY);
            return;
        };
        // Registering here is the fallback. Exclusion never depends on the
        // eager collection having happened, but enumeration does, so say so.
        if registry.register(V::KEY, own) {
            warn_once!(
                "`{}` was not collected at `AxisPlugin` build time and registered on first \
                 insert instead; it excludes correctly, but was missing from \
                 `AxisRegistry::variants` until now",
                V::KEY
            );
        }
        registry
            .members(V::KEY.axis())
            .iter()
            .map(|&(id, _)| id)
            .filter(|&id| id != own)
            .collect()
    };
    if stale.is_empty() {
        return;
    }

    // Removals are structural, so they cannot happen inside the hook itself.
    world.commands().queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
        // A variant inserted after us in the same batch already won.
        if !entity.contains_id(own) {
            return;
        }
        let present: Vec<ComponentId> = stale
            .into_iter()
            .filter(|&id| entity.contains_id(id))
            .collect();
        if !present.is_empty() {
            entity.remove_by_ids(&present);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_violations_are_detected() {
        assert!(axis_violation(1).is_none());
        assert!(axis_violation(0).is_some());
        assert!(axis_violation(2).is_some());
        assert!(axis_violation(3).is_some());
    }
}
