use std::any::{type_name, Any, TypeId};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::time::Duration;

use futures::stream::{self, LocalBoxStream};
use futures::StreamExt;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::command::Command;

pub type SubscriptionStream<U> = LocalBoxStream<'static, U>;

pub trait SubscriptionDriver: 'static {
    type Spec: Clone + Eq + fmt::Debug + 'static;
    type Update: 'static;

    fn subscribe(&self, spec: Self::Spec) -> SubscriptionStream<Self::Update>;
}

#[must_use]
pub struct Subscription<E, X> {
    entries: SmallVec<[SubscriptionEntry<E, X>; 4]>,
}

impl<E, X> Subscription<E, X> {
    pub fn none() -> Self {
        Self {
            entries: SmallVec::new(),
        }
    }

    pub fn custom<D, K, F>(key: K, spec: D::Spec, on_update: F) -> Self
    where
        D: SubscriptionDriver,
        K: Clone + Eq + Hash + fmt::Debug + 'static,
        F: FnMut(D::Update) -> Option<Command<E, X>> + 'static,
    {
        let mut entries = SmallVec::new();
        entries.push(SubscriptionEntry::new::<D, _, _>(key, spec, on_update));
        Self { entries }
    }

    pub fn batch<I>(subscriptions: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        let mut entries = SmallVec::new();
        for subscription in subscriptions {
            entries.extend(subscription.entries);
        }
        Self { entries }
    }

    pub fn and(mut self, mut other: Self) -> Self {
        self.entries.append(&mut other.entries);
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn into_entries(self) -> SmallVec<[SubscriptionEntry<E, X>; 4]> {
        self.entries
    }
}

impl<E, X> Default for Subscription<E, X> {
    fn default() -> Self {
        Self::none()
    }
}

impl<E, X> Subscription<E, X>
where
    E: Clone + 'static,
{
    pub fn every<K>(key: K, interval: Duration, event: E) -> Self
    where
        K: Clone + Eq + Hash + fmt::Debug + 'static,
    {
        assert!(
            !interval.is_zero(),
            "Subscription::every requires a non-zero interval"
        );

        Self::custom::<EveryDriver, _, _>(key, EverySpec { interval }, move |()| {
            Some(Command::event(event.clone()))
        })
    }
}

pub(crate) struct SubscriptionEntry<E, X> {
    key: SubscriptionKey,
    driver_id: TypeId,
    driver_name: &'static str,
    spec: SubscriptionSpec,
    mapper: Box<dyn ErasedSubscriptionMapper<E, X>>,
}

impl<E, X> SubscriptionEntry<E, X> {
    fn new<D, K, F>(key: K, spec: D::Spec, on_update: F) -> Self
    where
        D: SubscriptionDriver,
        K: Clone + Eq + Hash + fmt::Debug + 'static,
        F: FnMut(D::Update) -> Option<Command<E, X>> + 'static,
    {
        Self {
            key: SubscriptionKey::new(key),
            driver_id: TypeId::of::<D>(),
            driver_name: type_name::<D>(),
            spec: SubscriptionSpec::new(spec),
            mapper: Box::new(MapperFn::<D::Update, F>::new(on_update)),
        }
    }

    #[must_use]
    pub(crate) fn key(&self) -> &SubscriptionKey {
        &self.key
    }

    #[must_use]
    pub(crate) fn driver_id(&self) -> TypeId {
        self.driver_id
    }

    #[must_use]
    pub(crate) fn spec(&self) -> &SubscriptionSpec {
        &self.spec
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        SubscriptionKey,
        TypeId,
        &'static str,
        SubscriptionSpec,
        Box<dyn ErasedSubscriptionMapper<E, X>>,
    ) {
        (
            self.key,
            self.driver_id,
            self.driver_name,
            self.spec,
            self.mapper,
        )
    }
}

pub(crate) type ErasedSubscriptionStream = LocalBoxStream<'static, Box<dyn Any>>;

trait ErasedSubscriptionDriver: 'static {
    fn subscribe_erased(&self, spec: SubscriptionSpec) -> ErasedSubscriptionStream;
}

struct DriverBox<D>(D);

impl<D> ErasedSubscriptionDriver for DriverBox<D>
where
    D: SubscriptionDriver,
{
    fn subscribe_erased(&self, spec: SubscriptionSpec) -> ErasedSubscriptionStream {
        let spec = spec
            .into_any()
            .downcast::<D::Spec>()
            .unwrap_or_else(|_| panic!("subscription spec type mismatch for {}", type_name::<D>()));

        self.0
            .subscribe(*spec)
            .map(|update| Box::new(update) as Box<dyn Any>)
            .boxed_local()
    }
}

struct RegisteredDriver {
    driver: Box<dyn ErasedSubscriptionDriver>,
}

pub(crate) struct SubscriptionDrivers {
    inner: FxHashMap<TypeId, RegisteredDriver>,
}

impl SubscriptionDrivers {
    #[must_use]
    pub(crate) fn new() -> Self {
        let mut registry = Self {
            inner: FxHashMap::default(),
        };
        registry.register(EveryDriver);
        registry
    }

    pub(crate) fn register<D>(&mut self, driver: D)
    where
        D: SubscriptionDriver,
    {
        let previous = self.inner.insert(
            TypeId::of::<D>(),
            RegisteredDriver {
                driver: Box::new(DriverBox(driver)),
            },
        );
        assert!(
            previous.is_none(),
            "subscription driver {} is already registered",
            type_name::<D>()
        );
    }

    #[must_use]
    pub(crate) fn contains(&self, driver_id: TypeId) -> bool {
        self.inner.contains_key(&driver_id)
    }

    #[must_use]
    pub(crate) fn subscribe(
        &self,
        driver_id: TypeId,
        spec: SubscriptionSpec,
    ) -> Option<ErasedSubscriptionStream> {
        self.inner
            .get(&driver_id)
            .map(|registered| registered.driver.subscribe_erased(spec))
    }
}

impl Default for SubscriptionDrivers {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) trait ErasedSubscriptionMapper<E, X>: 'static {
    fn map(&mut self, update: Box<dyn Any>) -> Option<Command<E, X>>;
}

struct MapperFn<U, F> {
    inner: F,
    _marker: PhantomData<fn(U)>,
}

impl<U, F> MapperFn<U, F> {
    fn new(inner: F) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<E, X, U, F> ErasedSubscriptionMapper<E, X> for MapperFn<U, F>
where
    U: 'static,
    F: FnMut(U) -> Option<Command<E, X>> + 'static,
{
    fn map(&mut self, update: Box<dyn Any>) -> Option<Command<E, X>> {
        let update = update.downcast::<U>().unwrap_or_else(|_| {
            panic!(
                "subscription update type mismatch; expected {}",
                type_name::<U>()
            )
        });
        (self.inner)(*update)
    }
}

#[derive(Clone)]
pub(crate) struct SubscriptionKey {
    inner: Box<dyn SubscriptionKeyValue>,
}

impl SubscriptionKey {
    fn new<K>(key: K) -> Self
    where
        K: Clone + Eq + Hash + fmt::Debug + 'static,
    {
        Self {
            inner: Box::new(key),
        }
    }
}

impl PartialEq for SubscriptionKey {
    fn eq(&self, other: &Self) -> bool {
        self.inner.dyn_eq(other.inner.as_ref())
    }
}

impl Eq for SubscriptionKey {}

impl Hash for SubscriptionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.dyn_hash(state);
    }
}

impl fmt::Debug for SubscriptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

trait SubscriptionKeyValue: 'static {
    fn clone_box(&self) -> Box<dyn SubscriptionKeyValue>;
    fn dyn_eq(&self, other: &dyn SubscriptionKeyValue) -> bool;
    fn dyn_hash(&self, state: &mut dyn Hasher);
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
    fn as_any(&self) -> &dyn Any;
}

impl Clone for Box<dyn SubscriptionKeyValue> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl<K> SubscriptionKeyValue for K
where
    K: Clone + Eq + Hash + fmt::Debug + 'static,
{
    fn clone_box(&self) -> Box<dyn SubscriptionKeyValue> {
        Box::new(self.clone())
    }

    fn dyn_eq(&self, other: &dyn SubscriptionKeyValue) -> bool {
        other
            .as_any()
            .downcast_ref::<K>()
            .is_some_and(|other| self == other)
    }

    fn dyn_hash(&self, state: &mut dyn Hasher) {
        let mut inner = DefaultHasher::new();
        TypeId::of::<K>().hash(&mut inner);
        self.hash(&mut inner);
        state.write_u64(inner.finish());
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
pub(crate) struct SubscriptionSpec {
    inner: Box<dyn SubscriptionSpecValue>,
}

impl SubscriptionSpec {
    fn new<S>(spec: S) -> Self
    where
        S: Clone + Eq + fmt::Debug + 'static,
    {
        Self {
            inner: Box::new(spec),
        }
    }

    fn into_any(self) -> Box<dyn Any> {
        self.inner.into_any()
    }
}

impl PartialEq for SubscriptionSpec {
    fn eq(&self, other: &Self) -> bool {
        self.inner.dyn_eq(other.inner.as_ref())
    }
}

impl Eq for SubscriptionSpec {}

impl fmt::Debug for SubscriptionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

trait SubscriptionSpecValue: 'static {
    fn clone_box(&self) -> Box<dyn SubscriptionSpecValue>;
    fn dyn_eq(&self, other: &dyn SubscriptionSpecValue) -> bool;
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn as_any(&self) -> &dyn Any;
}

impl Clone for Box<dyn SubscriptionSpecValue> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl<S> SubscriptionSpecValue for S
where
    S: Clone + Eq + fmt::Debug + 'static,
{
    fn clone_box(&self) -> Box<dyn SubscriptionSpecValue> {
        Box::new(self.clone())
    }

    fn dyn_eq(&self, other: &dyn SubscriptionSpecValue) -> bool {
        other
            .as_any()
            .downcast_ref::<S>()
            .is_some_and(|other| self == other)
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EverySpec {
    interval: Duration,
}

struct EveryDriver;

impl SubscriptionDriver for EveryDriver {
    type Spec = EverySpec;
    type Update = ();

    fn subscribe(&self, spec: Self::Spec) -> SubscriptionStream<Self::Update> {
        stream::unfold(spec.interval, |interval| async move {
            crate::runtime::sleep(interval).await;
            Some(((), interval))
        })
        .boxed_local()
    }
}
