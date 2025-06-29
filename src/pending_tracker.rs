use kameo::{Actor, message::Message};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::time;
use uuid::Uuid;

pub struct Timeout(pub Uuid);

pub struct PendingMeta<K> {
    pub kind: K,
    pub who: Uuid,
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

    pub fn add(&mut self, kind: K, who: Uuid, dur: Duration) -> Uuid {
        let id = Uuid::new_v4();
        self.map.insert(
            id,
            PendingMeta {
                kind,
                who,
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

        id
    }

    pub fn take(&mut self, id: &Uuid) -> Option<PendingMeta<K>> {
        self.map.remove(id)
    }
}
