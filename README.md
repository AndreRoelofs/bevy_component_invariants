# bevy_component_invariants

Mutually exclusive Bevy components. `StateAxis` is a set of components, an `Entity` can only carry one of the components that are tagged with the same `StateAxis`.

The difference between this crate and [bevy-enum-components](https://github.com/Waridley/bevy-enum-components/tree/main) is that the `StateAxis` set is explicitly open. A new state can be added by a downstream crate or a third-party mod while still enforcing the invariants automatically.

```toml
[dependencies]
bevy_component_invariants = "0.19"
```

## Example

Let's say we have an item system in our game. An item can only be in one of the following states `OnGround` or `EquippedBy`.

```rust
use bevy_component_invariants::{AxisKey, AxisPlugin, StateAxis, variant_of};
use bevy_ecs::prelude::*;

pub struct ItemState;
impl StateAxis for ItemState {
    const KEY: AxisKey = AxisKey("core::item_state");
}

#[variant_of(ItemState)]
#[derive(Component, Clone, Copy)]
pub struct OnGround;

#[variant_of(ItemState, "equipped")]
#[derive(Component, Clone, Copy)]
pub struct EquippedBy(pub Entity);
```

Add `AxisPlugin` once. Inserting `EquippedBy` on an entity that is `OnGround` removes the `OnGround`. 

Let's say a downstream crate or a third-party mod adds some furniture that can display player's items. In that case, the mod author just has to declare:

```rust
#[variant_of(ItemState, "exhibit")]
#[derive(Component, Clone, Copy)]
pub struct ExhibitedBy;
```

Then if the player does the correct action, you can just call:

```rust
// ..
commands.entity(item).insert(ExhibitedBy(furniture));
// ..
```

The registration of all `StateAxis` happens eagerly at launch so other parts of the code can have access to the full list.

## Totality

Exclusivity of the `StateAxis` comes from just being a member of that axis. An properly spawned `Entity` will forever hold only one of the `StateAxis` child components.

## Other limitations

- **Queries are not exclusivity-aware.** `Query<&mut T, With<OnGround>>` and `Query<&mut T, With<EquippedBy>>` are disjoint in practice, but the scheduler does not know that, so it will not run them in parallel without explicit `Without` filters. Bevy is working on this [issue](https://github.com/bevyengine/bevy/issues/1481) with their own official implementation. 

## Bevy compatibility

| `bevy_component_invariants` | `bevy` |
| --- | --- |
| 0.19 | 0.19 |

## Example project

[**bevy_advanced_item_system**](https://github.com/AndreRoelofs/bevy_advanced_item_system) shows how an item system can be built on this crate. It features per-state views, a simple inventory system and status effect folding

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
