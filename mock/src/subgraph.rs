use futures::sync::mpsc::{channel, Receiver, Sender};
use graphql_parser;

use graph::prelude::*;
use graphql_parser::schema::Document;

/// A mock `SubgraphDeploymentProvider`.
pub struct MockSubgraphDeploymentProvider {
    logger: Logger,
    event_sink: Sender<SubgraphDeploymentProviderEvent>,
    event_stream: Option<Receiver<SubgraphDeploymentProviderEvent>>,
    _schemas: Vec<Schema>,
}

impl MockSubgraphDeploymentProvider {
    /// Creates a new mock `SubgraphDeploymentProvider`.
    pub fn new(logger: &Logger) -> Self {
        let (event_sink, event_stream) = channel(100);
        let id = SubgraphId::new("176dbd4fdeb8407b899be5d456ababc0").unwrap();
        MockSubgraphDeploymentProvider {
            logger: logger.new(o!("component" => "MockSubgraphDeploymentProvider")),
            event_sink,
            event_stream: Some(event_stream),
            _schemas: vec![Schema {
                id,
                document: graphql_parser::parse_schema(
                    "type User {
                           id: ID!
                           name: String!
                         }",
                )
                .unwrap(),
            }],
        }
    }

    /// Generates a bunch of mock subgraph provider events.
    fn generate_mock_events(&mut self) {
        info!(self.logger, "Generate mock events");

        let mock_subgraph = SubgraphManifest {
            id: SubgraphId::new("mocksubgraph").unwrap(),
            location: String::from("/tmp/example-data-source.yaml"),
            spec_version: String::from("0.1"),
            description: None,
            repository: None,
            schema: Schema {
                id: SubgraphId::new("exampleid").unwrap(),
                document: Document {
                    definitions: vec![],
                },
            },
            data_sources: vec![],
        };

        self.event_sink
            .clone()
            .send(SubgraphDeploymentProviderEvent::SubgraphStart(
                mock_subgraph,
            ))
            .wait()
            .unwrap();
    }
}

impl EventProducer<SubgraphDeploymentProviderEvent> for MockSubgraphDeploymentProvider {
    fn take_event_stream(
        &mut self,
    ) -> Option<Box<Stream<Item = SubgraphDeploymentProviderEvent, Error = ()> + Send>> {
        self.generate_mock_events();
        self.event_stream.take().map(|s| {
            Box::new(s) as Box<Stream<Item = SubgraphDeploymentProviderEvent, Error = ()> + Send>
        })
    }
}
