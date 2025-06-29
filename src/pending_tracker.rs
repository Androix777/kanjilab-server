use kameo::{Actor, message::Message};
use std::{
    collections::HashMap,
    marker::PhantomData,
    time::{Duration, Instant},
};
use tokio::time;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Ticket<K> {
    id: Uuid,
    _k: PhantomData<K>,
}

impl<K> Ticket<K> {
    fn new(id: Uuid) -> Self {
        Self {
            id,
            _k: PhantomData,
        }
    }
}

impl<K> From<Ticket<K>> for Uuid {
    fn from(t: Ticket<K>) -> Self {
        t.id
    }
}

impl<K> From<Uuid> for Ticket<K> {
    fn from(id: Uuid) -> Self {
        Ticket::new(id)
    }
}

pub struct Timeout(pub Uuid);

pub struct PendingMeta<K> {
    pub kind: K,
    pub sent: Instant,
    pub t_out: Duration,
}

pub struct PendingTracker<A, K>
where
    A: Actor + Message<Timeout, Reply = ()>,
    K: Copy + 'static,
{
    map: HashMap<Uuid, PendingMeta<K>>,
    actor: kameo::actor::WeakActorRef<A>,
}

impl<A, K> PendingTracker<A, K>
where
    A: Actor + Message<Timeout, Reply = ()>,
    K: Copy + 'static,
{
    pub fn new(actor: kameo::actor::WeakActorRef<A>) -> Self {
        Self {
            map: HashMap::new(),
            actor,
        }
    }

    pub fn add(&mut self, kind: K, dur: Duration) -> Ticket<K> {
        let id = Uuid::new_v4();
        self.map.insert(
            id,
            PendingMeta {
                kind,
                sent: Instant::now(),
                t_out: dur,
            },
        );

        let a = self.actor.clone();
        tokio::spawn(async move {
            time::sleep(dur).await;
            if let Some(r) = a.upgrade() {
                r.tell(Timeout(id)).await.ok();
            }
        });

        Ticket::new(id)
    }

    pub fn take(&mut self, ticket: Ticket<K>) -> Option<PendingMeta<K>> {
        self.map.remove(&ticket.id)
    }

    pub fn cancel(&mut self, ticket: Ticket<K>) -> bool {
        self.map.remove(&ticket.id).is_some()
    }
}
