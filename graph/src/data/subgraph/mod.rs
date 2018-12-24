use ethabi::Contract;
use failure;
use failure::{Error, SyncFailure};
use futures::stream;
use parity_wasm;
use parity_wasm::elements::Module;
use serde::de;
use serde::ser;
use serde_yaml;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use tokio::prelude::*;
use web3::types::Address;

use components::link_resolver::LinkResolver;
use data::query::QueryExecutionError;
use data::schema::Schema;

/// Rust representation of the GraphQL schema for a `SubgraphManifest`.
pub mod schema;

/// Deserialize an Address (with or without '0x' prefix).
fn deserialize_address<'de, D>(deserializer: D) -> Result<Address, D::Error>
where
    D: de::Deserializer<'de>,
{
    use serde::de::Error;

    let s: String = de::Deserialize::deserialize(deserializer)?;
    let address = s.trim_left_matches("0x");
    Address::from_str(address).map_err(D::Error::custom)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubgraphId(String);

impl SubgraphId {
    pub fn new(s: impl Into<String>) -> Result<Self, ()> {
        let s = s.into();

        // Enforce length limit
        if s.len() > 46 {
            return Err(());
        }

        // Check that the ID contains only allowed characters.
        if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(());
        }

        Ok(SubgraphId(s))
    }
}

impl fmt::Display for SubgraphId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ser::Serialize for SubgraphId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> de::Deserialize<'de> for SubgraphId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        SubgraphId::new(s.clone())
            .map_err(|()| de::Error::invalid_value(de::Unexpected::Str(&s), &"valid subgraph name"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubgraphName(String);

impl SubgraphName {
    pub fn new(s: impl Into<String>) -> Result<Self, ()> {
        let s = s.into();

        // Note: these validation rules must be kept consistent with the validation rules
        // implemented in any other components that rely on subgraph names.

        // Enforce length limits
        if s.len() < 1 || s.len() > 255 {
            return Err(());
        }

        // Check that the name contains only allowed characters.
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/')
        {
            return Err(());
        }

        // Check that name does not start or end with a special character
        // Note: we may in the future split names by '/' and do this check on each name "part".
        let first_char = s.chars().next().unwrap(); // length >= 1, checked earlier
        let last_char = s.chars().last().unwrap(); // length >= 1, checked earlier
        if !first_char.is_ascii_alphanumeric() || !last_char.is_ascii_alphanumeric() {
            return Err(());
        }

        Ok(SubgraphName(s))
    }
}

impl fmt::Display for SubgraphName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ser::Serialize for SubgraphName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> de::Deserialize<'de> for SubgraphName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        SubgraphName::new(s.clone())
            .map_err(|()| de::Error::invalid_value(de::Unexpected::Str(&s), &"valid subgraph name"))
    }
}

#[derive(Fail, Debug)]
pub enum SubgraphRegistrarError {
    #[fail(display = "subgraph resolve error: {}", _0)]
    ResolveError(SubgraphManifestResolveError),
    #[fail(display = "subgraph name not found: {}", _0)]
    NameNotFound(String),
    #[fail(display = "subgraph registrar internal query error: {}", _0)]
    QueryExecutionError(QueryExecutionError),
    #[fail(display = "subgraph registrar error: {}", _0)]
    Unknown(failure::Error),
}

impl From<QueryExecutionError> for SubgraphRegistrarError {
    fn from(e: QueryExecutionError) -> Self {
        SubgraphRegistrarError::QueryExecutionError(e)
    }
}

impl From<Error> for SubgraphRegistrarError {
    fn from(e: Error) -> Self {
        SubgraphRegistrarError::Unknown(e)
    }
}

#[derive(Fail, Debug)]
pub enum SubgraphDeploymentProviderError {
    #[fail(display = "subgraph resolve error: {}", _0)]
    ResolveError(SubgraphManifestResolveError),
    /// Occurs when attempting to remove a subgraph that's not hosted.
    #[fail(display = "subgraph with ID {} already running", _0)]
    AlreadyRunning(SubgraphId),
    #[fail(display = "subgraph with ID {} is not running", _0)]
    NotRunning(SubgraphId),
    /// Occurs when a subgraph's GraphQL schema is invalid.
    #[fail(display = "GraphQL schema error: {}", _0)]
    SchemaValidationError(failure::Error),
    #[fail(display = "subgraph provider error: {}", _0)]
    Unknown(failure::Error),
}

impl From<Error> for SubgraphDeploymentProviderError {
    fn from(e: Error) -> Self {
        SubgraphDeploymentProviderError::Unknown(e)
    }
}

/// Events emitted by [SubgraphDeploymentProvider](trait.SubgraphDeploymentProvider.html) implementations.
#[derive(Debug, PartialEq)]
pub enum SubgraphDeploymentProviderEvent {
    /// A subgraph with the given manifest should start processing.
    SubgraphStart(SubgraphManifest),
    /// The subgraph with the given ID should stop processing.
    SubgraphStop(SubgraphId),
}

#[derive(Fail, Debug)]
pub enum SubgraphManifestResolveError {
    #[fail(display = "parse error: {}", _0)]
    ParseError(serde_yaml::Error),
    #[fail(display = "subgraph is not UTF-8")]
    NonUtf8,
    #[fail(display = "subgraph is not valid YAML")]
    InvalidFormat,
    #[fail(display = "resolve error: {}", _0)]
    ResolveError(failure::Error),
}

impl From<serde_yaml::Error> for SubgraphManifestResolveError {
    fn from(e: serde_yaml::Error) -> Self {
        SubgraphManifestResolveError::ParseError(e)
    }
}

/// IPLD link.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct Link {
    #[serde(rename = "/")]
    pub link: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct SchemaData {
    pub file: Link,
}

impl SchemaData {
    pub fn resolve(
        self,
        id: SubgraphId,
        resolver: &impl LinkResolver,
    ) -> impl Future<Item = Schema, Error = failure::Error> + Send {
        resolver
            .cat(&self.file)
            .and_then(|schema_bytes| Schema::parse(&String::from_utf8(schema_bytes)?, id))
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct Source {
    #[serde(deserialize_with = "deserialize_address")]
    pub address: Address,
    pub abi: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct UnresolvedMappingABI {
    pub name: String,
    pub file: Link,
}

#[derive(Clone, Debug)]
pub struct MappingABI {
    pub name: String,
    pub contract: Contract,
    pub link: Link,
}

impl UnresolvedMappingABI {
    pub fn resolve(
        self,
        resolver: &impl LinkResolver,
    ) -> impl Future<Item = MappingABI, Error = failure::Error> + Send {
        resolver.cat(&self.file).and_then(|contract_bytes| {
            let contract = Contract::load(&*contract_bytes).map_err(SyncFailure::new)?;
            Ok(MappingABI {
                name: self.name,
                contract,
                link: self.file,
            })
        })
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct MappingEventHandler {
    pub event: String,
    pub handler: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedMapping {
    pub kind: String,
    pub api_version: String,
    pub language: String,
    pub entities: Vec<String>,
    pub abis: Vec<UnresolvedMappingABI>,
    pub event_handlers: Vec<MappingEventHandler>,
    pub file: Link,
}

// Avoid deriving `Clone` because cloning a `Module` is expensive.
#[derive(Debug)]
pub struct Mapping {
    pub kind: String,
    pub api_version: String,
    pub language: String,
    pub entities: Vec<String>,
    pub abis: Vec<MappingABI>,
    pub event_handlers: Vec<MappingEventHandler>,
    pub runtime: Module,
    pub link: Link,
}

impl UnresolvedMapping {
    pub fn resolve(
        self,
        resolver: &impl LinkResolver,
    ) -> impl Future<Item = Mapping, Error = failure::Error> + Send {
        let UnresolvedMapping {
            kind,
            api_version,
            language,
            entities,
            abis,
            event_handlers,
            file: link,
        } = self;

        // resolve each abi
        stream::futures_ordered(
            abis.into_iter()
                .map(|unresolved_abi| unresolved_abi.resolve(resolver)),
        )
        .collect()
        .join(
            resolver
                .cat(&link)
                .and_then(|module_bytes| Ok(parity_wasm::deserialize_buffer(&module_bytes)?)),
        )
        .map(|(abis, runtime)| Mapping {
            kind,
            api_version,
            language,
            entities,
            abis,
            event_handlers,
            runtime,
            link,
        })
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct BaseDataSource<M> {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub source: Source,
    pub mapping: M,
}

pub type UnresolvedDataSource = BaseDataSource<UnresolvedMapping>;
pub type DataSource = BaseDataSource<Mapping>;

impl UnresolvedDataSource {
    pub fn resolve(
        self,
        resolver: &impl LinkResolver,
    ) -> impl Future<Item = DataSource, Error = failure::Error> {
        let UnresolvedDataSource {
            kind,
            network,
            name,
            source,
            mapping,
        } = self;
        mapping.resolve(resolver).map(|mapping| DataSource {
            kind,
            network,
            name,
            source,
            mapping,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseSubgraphManifest<S, D> {
    pub id: SubgraphId,
    pub location: String,
    pub spec_version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub schema: S,
    pub data_sources: Vec<D>,
}

/// Consider two subgraphs to be equal if they come from the same IPLD link.
impl<S, D> PartialEq for BaseSubgraphManifest<S, D> {
    fn eq(&self, other: &Self) -> bool {
        self.location == other.location
    }
}

pub type UnresolvedSubgraphManifest = BaseSubgraphManifest<SchemaData, UnresolvedDataSource>;
pub type SubgraphManifest = BaseSubgraphManifest<Schema, DataSource>;

impl SubgraphManifest {
    /// Entry point for resolving a subgraph definition.
    /// Right now the only supported links are of the form:
    /// `/ipfs/QmUmg7BZC1YP1ca66rRtWKxpXp77WgVHrnv263JtDuvs2k`
    pub fn resolve(
        link: Link,
        resolver: Arc<impl LinkResolver>,
    ) -> impl Future<Item = Self, Error = SubgraphManifestResolveError> + Send {
        resolver
            .cat(&link)
            .map_err(SubgraphManifestResolveError::ResolveError)
            .and_then(move |file_bytes| {
                let file = String::from_utf8(file_bytes.to_vec())
                    .map_err(|_| SubgraphManifestResolveError::NonUtf8)?;
                let mut raw: serde_yaml::Value = serde_yaml::from_str(&file)?;
                {
                    let raw_mapping = raw
                        .as_mapping_mut()
                        .ok_or(SubgraphManifestResolveError::InvalidFormat)?;

                    // Inject the IPFS hash as the ID of the subgraph
                    // into the definition.
                    raw_mapping.insert(
                        serde_yaml::Value::from("id"),
                        serde_yaml::Value::from(link.link.trim_left_matches("/ipfs/")),
                    );

                    // Inject the IPFS link as the location of the data
                    // source into the definition
                    raw_mapping.insert(
                        serde_yaml::Value::from("location"),
                        serde_yaml::Value::from(link.link),
                    );
                }
                // Parse the YAML data into an UnresolvedSubgraphManifest
                let unresolved: UnresolvedSubgraphManifest = serde_yaml::from_value(raw)?;
                Ok(unresolved)
            })
            .and_then(move |unresolved| {
                unresolved
                    .resolve(&*resolver)
                    .map_err(SubgraphManifestResolveError::ResolveError)
            })
    }
}

impl UnresolvedSubgraphManifest {
    pub fn resolve(
        self,
        resolver: &impl LinkResolver,
    ) -> impl Future<Item = SubgraphManifest, Error = failure::Error> {
        let UnresolvedSubgraphManifest {
            id,
            location,
            spec_version,
            description,
            repository,
            schema,
            data_sources,
        } = self;

        // resolve each data set
        stream::futures_ordered(
            data_sources
                .into_iter()
                .map(|data_set| data_set.resolve(resolver)),
        )
        .collect()
        .join(schema.resolve(id.clone(), resolver))
        .map(|(data_sources, schema)| SubgraphManifest {
            id,
            location,
            spec_version,
            description,
            repository,
            schema,
            data_sources,
        })
    }
}
