use borsh::BorshSerialize;
pub use clap::{Parser, Subcommand};
use database::ScyllaStorageManager;
use near_indexer_primitives::types::{BlockReference, Finality};
use near_jsonrpc_client::{methods, JsonRpcClient};
use num_traits::ToPrimitive;
use scylla::prepared_statement::PreparedStatement;
use tracing_subscriber::EnvFilter;

/// NEAR Indexer for Explorer
/// Watches for stream of blocks from the chain
#[derive(Parser, Debug)]
#[clap(
    version,
    author,
    about,
    setting(clap::AppSettings::DisableHelpSubcommand),
    setting(clap::AppSettings::PropagateVersion),
    setting(clap::AppSettings::NextLineHelp)
)]
pub(crate) struct Opts {
    /// Indexer ID to handle meta data about the instance
    #[clap(long, env)]
    pub indexer_id: String,
    /// Port for metrics server
    #[clap(long, default_value = "8080", env)]
    pub port: u16,
    /// ScyllaDB connection string. Default: "127.0.0.1:9042"
    #[clap(long, default_value = "127.0.0.1:9042", env)]
    pub scylla_url: String,
    /// ScyllaDB keyspace
    #[clap(long, default_value = "tx_indexer", env)]
    pub scylla_keyspace: String,
    /// ScyllaDB user(login)
    #[clap(long, env)]
    pub scylla_user: Option<String>,
    /// ScyllaDB password
    #[clap(long, env)]
    pub scylla_password: Option<String>,
    /// Chain ID: testnet or mainnet
    #[clap(subcommand)]
    pub chain_id: ChainId,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ChainId {
    #[clap(subcommand)]
    Mainnet(StartOptions),
    #[clap(subcommand)]
    Testnet(StartOptions),
}

#[allow(clippy::enum_variant_names)]
#[derive(Subcommand, Debug, Clone)]
pub enum StartOptions {
    FromBlock { height: u64 },
    FromInterruption,
    FromLatest,
}

impl Opts {
    /// Returns [StartOptions] for current [Opts]
    pub fn start_options(&self) -> &StartOptions {
        match &self.chain_id {
            ChainId::Mainnet(start_options) | ChainId::Testnet(start_options) => start_options,
        }
    }

    pub fn rpc_url(&self) -> &str {
        match self.chain_id {
            ChainId::Mainnet(_) => "https://rpc.mainnet.near.org",
            ChainId::Testnet(_) => "https://rpc.testnet.near.org",
        }
    }
}

impl Opts {
    pub async fn to_lake_config(
        &self,
        scylladb_session: &std::sync::Arc<scylla::Session>,
    ) -> anyhow::Result<near_lake_framework::LakeConfig> {
        let config_builder = near_lake_framework::LakeConfigBuilder::default();

        Ok(match &self.chain_id {
            ChainId::Mainnet(_) => config_builder
                .mainnet()
                .start_block_height(get_start_block_height(self, scylladb_session).await?),
            ChainId::Testnet(_) => config_builder
                .testnet()
                .start_block_height(get_start_block_height(self, scylladb_session).await?),
        }
        .build()
        .expect("Failed to build LakeConfig"))
    }
}

async fn get_start_block_height(
    opts: &Opts,
    scylladb_session: &std::sync::Arc<scylla::Session>,
) -> anyhow::Result<u64> {
    match opts.start_options() {
        StartOptions::FromBlock { height } => Ok(*height),
        StartOptions::FromInterruption => {
            let row = scylladb_session
                .query(
                    "SELECT last_processed_block_height FROM tx_indexer.meta WHERE indexer_id = ?",
                    (&opts.indexer_id,),
                )
                .await?
                .single_row();

            if let Ok(row) = row {
                let (block_height,): (num_bigint::BigInt,) =
                    row.into_typed::<(num_bigint::BigInt,)>()?;
                Ok(block_height
                    .to_u64()
                    .expect("Failed to convert BigInt to u64"))
            } else {
                Ok(final_block_height(opts).await)
            }
        }
        StartOptions::FromLatest => Ok(final_block_height(opts).await),
    }
}

async fn final_block_height(opts: &Opts) -> u64 {
    let client = JsonRpcClient::connect(opts.rpc_url().to_string());
    let request = methods::block::RpcBlockRequest {
        block_reference: BlockReference::Finality(Finality::Final),
    };

    let latest_block = client.call(request).await.unwrap();

    latest_block.header.height
}

pub fn init_tracing() -> anyhow::Result<()> {
    let mut env_filter = EnvFilter::new("near_lake_framework=info,tx_indexer=info,storage_tx=info");

    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    eprintln!("Ignoring directive `{}`: {}", s, err);
                    None
                }
            }) {
                env_filter = env_filter.add_directive(directive);
            }
        }
    }

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr);

    if std::env::var("ENABLE_JSON_LOGS").is_ok() {
        subscriber.json().init();
    } else {
        subscriber.compact().init();
    }

    Ok(())
}

pub(crate) struct ScyllaDBManager {
    scylla_session: std::sync::Arc<scylla::Session>,
    add_transaction: PreparedStatement,
    add_receipt: PreparedStatement,
    update_meta: PreparedStatement,
}

#[async_trait::async_trait]
impl ScyllaStorageManager for ScyllaDBManager {
    async fn create_tables(scylla_db_session: &scylla::Session) -> anyhow::Result<()> {
        scylla_db_session.use_keyspace("tx_indexer", false).await?;
        scylla_db_session
            .query(
                "CREATE TABLE IF NOT EXISTS transactions_details (
                transaction_hash varchar,
                block_height varint,
                account_id varchar,
                transaction_details BLOB,
                PRIMARY KEY (transaction_hash, block_height)
            ) WITH CLUSTERING ORDER BY (block_height DESC)
            ",
                &[],
            )
            .await?;

        scylla_db_session
            .query(
                "CREATE TABLE IF NOT EXISTS receipts_map (
                receipt_id varchar,
                block_height varint,
                parent_transaction_hash varchar,
                shard_id varint,
                PRIMARY KEY (receipt_id)
            )
            ",
                &[],
            )
            .await?;

        scylla_db_session
            .query(
                "
                CREATE TABLE IF NOT EXISTS meta (
                    indexer_id varchar PRIMARY KEY,
                    last_processed_block_height varint
                )
            ",
                &[],
            )
            .await?;

        Ok(())
    }

    async fn create_keyspace(scylla_db_session: &scylla::Session) -> anyhow::Result<()> {
        scylla_db_session.query(
            "CREATE KEYSPACE IF NOT EXISTS tx_indexer WITH REPLICATION = {'class': 'SimpleStrategy', 'replication_factor': 1}",
            &[]
        ).await?;
        Ok(())
    }

    async fn prepare(
        scylla_db_session: std::sync::Arc<scylla::Session>,
    ) -> anyhow::Result<Box<Self>> {
        Ok(Box::new(Self {
            scylla_session: scylla_db_session.clone(),
            add_transaction: Self::prepare_query(
                &scylla_db_session,
                "INSERT INTO tx_indexer.transactions_details
                    (transaction_hash, block_height, account_id, transaction_details)
                    VALUES(?, ?, ?, ?)",
            )
            .await?,
            add_receipt: Self::prepare_query(
                &scylla_db_session,
                "INSERT INTO tx_indexer.receipts_map
                    (receipt_id, block_height, parent_transaction_hash, shard_id)
                    VALUES(?, ?, ?, ?)",
            )
            .await?,
            update_meta: Self::prepare_query(
                &scylla_db_session,
                "INSERT INTO tx_indexer.meta
                    (indexer_id, last_processed_block_height)
                    VALUES (?, ?)",
            )
            .await?,
        }))
    }
}

impl ScyllaDBManager {
    pub(crate) async fn scylla_session(&self) -> std::sync::Arc<scylla::Session> {
        self.scylla_session.clone()
    }

    pub async fn add_transaction(
        &self,
        transaction: readnode_primitives::TransactionDetails,
        block_height: u64,
    ) -> anyhow::Result<()> {
        let transaction_details = transaction
            .try_to_vec()
            .expect("Failed to borsh-serialize the Transaction");
        Self::execute_prepared_query(
            &self.scylla_session,
            &self.add_transaction,
            (
                transaction.transaction.hash.to_string(),
                num_bigint::BigInt::from(block_height),
                transaction.transaction.signer_id.to_string(),
                &transaction_details,
            ),
        )
        .await?;
        Ok(())
    }

    pub async fn add_receipt(
        &self,
        receipt_id: &str,
        parent_tx_hash: &str,
        block_height: u64,
        shard_id: u64,
    ) -> anyhow::Result<()> {
        Self::execute_prepared_query(
            &self.scylla_session,
            &self.add_receipt,
            (
                receipt_id,
                num_bigint::BigInt::from(block_height),
                parent_tx_hash,
                num_bigint::BigInt::from(shard_id),
            ),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn update_meta(
        &self,
        indexer_id: &str,
        block_height: u64,
    ) -> anyhow::Result<()> {
        Self::execute_prepared_query(
            &self.scylla_session,
            &self.update_meta,
            (indexer_id, num_bigint::BigInt::from(block_height)),
        )
        .await?;
        Ok(())
    }
}