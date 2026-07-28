extern crate self as bevy_component_invariants;

use std::collections::HashMap;
use std::fmt;

use bevy_app::prelude::*;
use bevy_ecs::component::ComponentId;
use bevy_ecs::event::EntityComponentsTrigger;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::observer::IntoObserver;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::system::EntityCommands;
use bevy_ecs::world::{DeferredWorld, EntityWorldMut};
use bevy_log::prelude::*;

pub use bevy_component_invariants_macro::variant_of;

pub struct AxisPlugin;

impl Plugin for AxisPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AxisRegistry>();
        for entry in __private::inventory::iter::<VariantRegistration> {
            let id = (entry.register)(app.world_mut());
            app.world_mut()
                .resource_mut::<AxisRegistry>()
                .register(entry.key, id);
        }
    }
}

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

pub trait StateAxis: 'static {
    const KEY: AxisKey;
}

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

pub trait VariantOf: Component + __private::Sealed {
    type Axis: StateAxis;
    const KEY: VariantKey;
}

#[doc(hidden)]
pub struct VariantRegistration {
    pub key: VariantKey,
    pub register: fn(&mut World) -> ComponentId,
}

__private::inventory::collect!(VariantRegistration);

#[doc(hidden)]
pub fn register_component_of<V: VariantOf>(world: &mut World) -> ComponentId {
    world.register_component::<V>()
}

#[doc(hidden)]
pub mod __private {
    pub trait Sealed {}

    pub use ::inventory;
}

#[derive(Resource, Default)]
pub struct AxisRegistry(HashMap<AxisKey, Vec<(ComponentId, VariantKey)>>);

impl AxisRegistry {
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

    pub fn axes(&self) -> impl Iterator<Item = AxisKey> + '_ {
        self.0.keys().copied()
    }

    pub fn component_ids(&self, axis: AxisKey) -> impl Iterator<Item = ComponentId> + '_ {
        self.members(axis).iter().map(|&(id, _)| id)
    }

    pub fn variant_of_id(&self, axis: AxisKey, id: ComponentId) -> Option<VariantKey> {
        self.members(axis)
            .iter()
            .find(|&&(known, _)| known == id)
            .map(|&(_, key)| key)
    }

    pub fn variants(&self, axis: AxisKey) -> impl Iterator<Item = VariantKey> + '_ {
        self.members(axis).iter().map(|&(_, key)| key)
    }

    pub fn variant_by_name(&self, axis: AxisKey, name: &str) -> Option<VariantKey> {
        self.variants(axis).find(|key| key.name() == name)
    }

    pub fn component_id_of(&self, key: VariantKey) -> Option<ComponentId> {
        self.members(key.axis())
            .iter()
            .find(|&&(_, known)| known == key)
            .map(|&(id, _)| id)
    }

    pub fn variant_on<A: StateAxis>(&self, entity: EntityRef) -> Option<VariantKey> {
        self.variant_on_key(entity, A::KEY)
    }

    pub fn variant_on_key(&self, entity: EntityRef, axis: AxisKey) -> Option<VariantKey> {
        self.members(axis)
            .iter()
            .find(|&&(id, _)| entity.contains_id(id))
            .map(|&(_, key)| key)
    }

    pub fn count_on<A: StateAxis>(&self, entity: EntityRef) -> usize {
        self.members(A::KEY)
            .iter()
            .filter(|&&(id, _)| entity.contains_id(id))
            .count()
    }
}

pub trait AxisAppExt {
    fn require_total_axis<A: StateAxis, F: QueryFilter + 'static>(&mut self) -> &mut Self;

    fn add_axis_observer<A: StateAxis, M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self;
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

    fn add_axis_observer<A: StateAxis, M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self {
        if !self.is_plugin_added::<AxisPlugin>() {
            self.add_plugins(AxisPlugin);
        }
        let members = snapshot::<A>(self.world());
        self.world_mut()
            .spawn(observer.into_observer().with_components(members));
        self
    }
}

pub trait AxisObserveExt {
    fn observe_axis<A: StateAxis, M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self;
}

impl AxisObserveExt for EntityCommands<'_> {
    fn observe_axis<A: StateAxis, M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self {
        let target = self.id();
        self.commands().queue(move |world: &mut World| {
            world.entity_mut(target).observe_axis::<A, M>(observer);
        });
        self
    }
}

impl AxisObserveExt for EntityWorldMut<'_> {
    fn observe_axis<A: StateAxis, M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self {
        let target = self.id();
        self.world_scope(|world| {
            let members = snapshot::<A>(world);
            world.spawn(
                observer
                    .into_observer()
                    .with_components(members)
                    .with_entity(target),
            );
        });
        self
    }
}

fn snapshot<A: StateAxis>(world: &World) -> Vec<ComponentId> {
    world
        .resource::<AxisRegistry>()
        .component_ids(A::KEY)
        .collect()
}

pub trait AxisTriggerExt {
    fn axis_variant<A: StateAxis>(&self, axes: &AxisRegistry) -> Option<VariantKey>;
}

impl<'t, E, B> AxisTriggerExt for On<'_, 't, E, B>
where
    E: Event<Trigger<'t> = EntityComponentsTrigger<'t>>,
    B: Bundle,
{
    fn axis_variant<A: StateAxis>(&self, axes: &AxisRegistry) -> Option<VariantKey> {
        self.trigger()
            .components
            .iter()
            .find_map(|&id| axes.variant_of_id(A::KEY, id))
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

pub fn enforce_axis<V: VariantOf>(mut world: DeferredWorld, context: HookContext) {
    let (entity, own) = (context.entity, context.component_id);

    let stale: Vec<ComponentId> = {
        let Some(mut registry) = world.get_resource_mut::<AxisRegistry>() else {
            warn_once!("`AxisPlugin` is missing; `{}` excludes nothing", V::KEY);
            return;
        };
        if registry.register(V::KEY, own) {
            warn_once!(
                "`{}` was not collected at `AxisPlugin` build time and registered on first \
                 insert instead; it excludes correctly, but was missing from \
                 `AxisRegistry::variants` until now, so anything built from an earlier reading \
                 of `{}` — an `add_axis_observer`, a saved state listing — predates it",
                V::KEY,
                V::KEY.axis()
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

    world.commands().queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
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

    struct Stance;
    impl StateAxis for Stance {
        const KEY: AxisKey = AxisKey("test::stance");
    }

    #[variant_of(Stance, "alpha")]
    #[derive(Component, Clone, Copy)]
    struct Alpha;

    #[variant_of(Stance, "beta")]
    #[derive(Component, Clone, Copy)]
    struct Beta;

    #[derive(Component, Clone, Copy)]
    struct OffAxis;

    #[derive(Resource, Default)]
    struct Seen(Vec<VariantKey>);

    fn record(insert: On<Insert>, axes: Res<AxisRegistry>, mut seen: ResMut<Seen>) {
        if let Some(variant) = insert.axis_variant::<Stance>(&axes) {
            seen.0.push(variant);
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(AxisPlugin).init_resource::<Seen>();
        app
    }

    #[test]
    fn axis_violations_are_detected() {
        assert!(axis_violation(1).is_none());
        assert!(axis_violation(0).is_some());
        assert!(axis_violation(2).is_some());
        assert!(axis_violation(3).is_some());
    }

    #[test]
    fn one_observer_covers_every_variant() {
        let mut app = test_app();
        app.add_axis_observer::<Stance, _>(record);

        let entity = app.world_mut().spawn(Alpha).id();
        app.world_mut().entity_mut(entity).insert(Beta);
        app.world_mut().entity_mut(entity).insert(Alpha);

        assert_eq!(
            app.world().resource::<Seen>().0,
            vec![Alpha::KEY, Beta::KEY, Alpha::KEY]
        );
    }

    #[test]
    fn components_off_the_axis_do_not_reach_the_observer() {
        let mut app = test_app();
        app.add_axis_observer::<Stance, _>(record);

        app.world_mut().spawn(OffAxis);

        assert!(app.world().resource::<Seen>().0.is_empty());
    }

    #[test]
    fn the_trigger_names_the_arriving_variant_not_the_outgoing_one() {
        #[derive(Resource, Default)]
        struct Reading {
            from_trigger: Option<VariantKey>,
            from_registry: Option<VariantKey>,
        }

        let mut app = test_app();
        app.init_resource::<Reading>();
        app.add_axis_observer::<Stance, _>(
            |insert: On<Insert>,
             world: Query<EntityRef>,
             axes: Res<AxisRegistry>,
             mut commands: Commands| {
                let entity = insert.event().entity;
                let from_trigger = insert.axis_variant::<Stance>(&axes);
                let from_registry = world
                    .get(entity)
                    .ok()
                    .and_then(|entity| axes.variant_on::<Stance>(entity));
                commands.queue(move |world: &mut World| {
                    *world.resource_mut::<Reading>() = Reading {
                        from_trigger,
                        from_registry,
                    };
                });
            },
        );

        let entity = app.world_mut().spawn(Alpha).id();
        app.world_mut().entity_mut(entity).insert(Beta);

        let reading = app.world().resource::<Reading>();
        assert_eq!(
            reading.from_trigger,
            Some(Beta::KEY),
            "the event names what was inserted"
        );
        assert_eq!(
            reading.from_registry,
            Some(Alpha::KEY),
            "while `variant_on` still sees the sibling exclusion has not yet removed"
        );
    }

    #[test]
    fn an_entity_observer_watches_one_entity_along_the_axis() {
        let mut app = test_app();
        let watched = app.world_mut().spawn_empty().id();
        app.world_mut()
            .entity_mut(watched)
            .observe_axis::<Stance, _>(record);
        let ignored = app.world_mut().spawn_empty().id();

        app.world_mut().entity_mut(ignored).insert(Alpha);
        app.world_mut().entity_mut(watched).insert(Beta);

        assert_eq!(app.world().resource::<Seen>().0, vec![Beta::KEY]);
    }
}
