use std::collections::HashMap;

use graph::components::store::*;
use graph::prelude::*;
use graph::web3::types::H256;

/// A mock `ChainHeadUpdateListener`
pub struct MockChainHeadUpdateListener {}

impl ChainHeadUpdateListener for MockChainHeadUpdateListener {
    fn start(&mut self) {}
}

impl EventProducer<ChainHeadUpdate> for MockChainHeadUpdateListener {
    fn take_event_stream(
        &mut self,
    ) -> Option<Box<Stream<Item = ChainHeadUpdate, Error = ()> + Send>> {
        unimplemented!();
    }
}

pub struct MockStore {
    entities: Vec<Entity>,
    schemas: HashMap<SubgraphId, Schema>,
}

impl MockStore {
    /// Creates a new mock `Store`.
    pub fn new(schemas: Vec<(SubgraphId, Schema)>) -> Self {
        // Create a few test entities
        let mut entities = vec![];
        for (i, name) in ["Joe", "Jeff", "Linda"].iter().enumerate() {
            let mut entity = Entity::new();
            entity.insert("id".to_string(), Value::String(i.to_string()));
            entity.insert("name".to_string(), Value::String(name.to_string()));
            entities.push(entity);
        }

        MockStore {
            entities,
            schemas: schemas.into_iter().collect(),
        }
    }
}

impl Store for MockStore {
    fn get(&self, key: EntityKey) -> Result<Option<Entity>, QueryExecutionError> {
        if key.entity_type == "User" {
            self.entities
                .iter()
                .find(|entity| {
                    let id = entity.get("id").unwrap();
                    match *id {
                        Value::String(ref s) => s == &key.entity_id,
                        _ => false,
                    }
                })
                .map(|entity| Some(entity.clone()))
                .ok_or_else(|| unimplemented!())
        } else {
            unimplemented!()
        }
    }

    fn find(&self, _query: EntityQuery) -> Result<Vec<Entity>, QueryExecutionError> {
        Ok(self.entities.clone())
    }

    fn block_ptr(&self, _: SubgraphId) -> Result<EthereumBlockPointer, Error> {
        unimplemented!();
    }

    fn set_block_ptr_with_no_changes(
        &self,
        _: SubgraphId,
        _: EthereumBlockPointer,
        _: EthereumBlockPointer,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    fn transact_block_operations(
        &self,
        _: SubgraphId,
        _: EthereumBlockPointer,
        _: EthereumBlockPointer,
        _: Vec<EntityOperation>,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    fn apply_entity_operations(
        &self,
        _: Vec<EntityOperation>,
        _: EventSource,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn revert_block_operations(
        &self,
        _: SubgraphId,
        _: EthereumBlockPointer,
        _: EthereumBlockPointer,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    fn subscribe(&self, _: Vec<SubgraphEntityPair>) -> EntityChangeStream {
        unimplemented!();
    }

    fn count_entities(&self, _: SubgraphId) -> Result<u64, Error> {
        unimplemented!();
    }
}

impl SubgraphDeploymentStore for MockStore {
    fn is_deployed(&self, subgraph_id: &SubgraphId) -> Result<bool, Error> {
        Ok(self.schemas.keys().any(|id| subgraph_id == id))
    }

    fn subgraph_schema(&self, subgraph_id: SubgraphId) -> Result<Schema, Error> {
        Ok(self.schemas.get(&subgraph_id).unwrap().clone())
    }
}

impl ChainStore for MockStore {
    type ChainHeadUpdateListener = MockChainHeadUpdateListener;

    fn genesis_block_ptr(&self) -> Result<EthereumBlockPointer, Error> {
        unimplemented!();
    }

    fn upsert_blocks<'a, B, E>(&self, _: B) -> Box<Future<Item = (), Error = E> + Send + 'a>
    where
        B: Stream<Item = EthereumBlock, Error = E> + Send + 'a,
        E: From<Error> + Send + 'a,
    {
        unimplemented!();
    }

    fn attempt_chain_head_update(&self, _: u64) -> Result<Vec<H256>, Error> {
        unimplemented!();
    }

    fn chain_head_updates(&self) -> Self::ChainHeadUpdateListener {
        unimplemented!();
    }

    fn chain_head_ptr(&self) -> Result<Option<EthereumBlockPointer>, Error> {
        unimplemented!();
    }

    fn block(&self, _: H256) -> Result<Option<EthereumBlock>, Error> {
        unimplemented!();
    }

    fn ancestor_block(
        &self,
        _: EthereumBlockPointer,
        _: u64,
    ) -> Result<Option<EthereumBlock>, Error> {
        unimplemented!();
    }
}

pub struct FakeStore;

impl Store for FakeStore {
    fn get(&self, _: EntityKey) -> Result<Option<Entity>, QueryExecutionError> {
        unimplemented!();
    }

    fn find(&self, _: EntityQuery) -> Result<Vec<Entity>, QueryExecutionError> {
        unimplemented!();
    }

    fn block_ptr(&self, _: SubgraphId) -> Result<EthereumBlockPointer, Error> {
        unimplemented!();
    }

    fn set_block_ptr_with_no_changes(
        &self,
        _: SubgraphId,
        _: EthereumBlockPointer,
        _: EthereumBlockPointer,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    fn transact_block_operations(
        &self,
        _: SubgraphId,
        _: EthereumBlockPointer,
        _: EthereumBlockPointer,
        _: Vec<EntityOperation>,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    fn apply_entity_operations(
        &self,
        _: Vec<EntityOperation>,
        _: EventSource,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn revert_block_operations(
        &self,
        _: SubgraphId,
        _: EthereumBlockPointer,
        _: EthereumBlockPointer,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    fn subscribe(&self, _: Vec<SubgraphEntityPair>) -> EntityChangeStream {
        unimplemented!();
    }

    fn count_entities(&self, _: SubgraphId) -> Result<u64, Error> {
        unimplemented!();
    }
}

impl ChainStore for FakeStore {
    type ChainHeadUpdateListener = MockChainHeadUpdateListener;

    fn genesis_block_ptr(&self) -> Result<EthereumBlockPointer, Error> {
        unimplemented!();
    }

    fn upsert_blocks<'a, B, E>(&self, _: B) -> Box<Future<Item = (), Error = E> + Send + 'a>
    where
        B: Stream<Item = EthereumBlock, Error = E> + Send + 'a,
        E: From<Error> + Send + 'a,
    {
        unimplemented!();
    }

    fn attempt_chain_head_update(&self, _: u64) -> Result<Vec<H256>, Error> {
        unimplemented!();
    }

    fn chain_head_updates(&self) -> Self::ChainHeadUpdateListener {
        unimplemented!();
    }

    fn chain_head_ptr(&self) -> Result<Option<EthereumBlockPointer>, Error> {
        unimplemented!();
    }

    fn block(&self, _: H256) -> Result<Option<EthereumBlock>, Error> {
        unimplemented!();
    }

    fn ancestor_block(
        &self,
        _: EthereumBlockPointer,
        _: u64,
    ) -> Result<Option<EthereumBlock>, Error> {
        unimplemented!();
    }
}
