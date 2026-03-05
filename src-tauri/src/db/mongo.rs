use anyhow::{Context, Result};
use mongodb::{Client, Database, Collection};
use once_cell::sync::OnceCell;
use crate::db::models::{MediaDoc, PersonDoc, SearchHistoryDoc, VectorEmbeddingDoc};

static MONGO_CLIENT: OnceCell<Client> = OnceCell::new();
const DB_NAME: &str = "auraseek";

pub struct MongoDb {
    pub db: Database,
}

impl MongoDb {
    pub async fn connect(uri: &str) -> Result<Self> {
        let client = Client::with_uri_str(uri)
            .await
            .context("Failed to connect to MongoDB")?;

        // Store globally for reuse
        let _ = MONGO_CLIENT.set(client.clone());

        let db = client.database(DB_NAME);

        // Explicitly ping the auth to verify credentials (prevents silent auth failure)
        db.run_command(mongodb::bson::doc! { "ping": 1 })
            .await
            .context("MongoDB authentication or connection failed. Please check your username/password and try adding ?authSource=admin to the URI.")?;

        // Ensure indexes
        Self::ensure_indexes(&db).await?;

        Ok(Self { db })
    }

    async fn ensure_indexes(db: &Database) -> Result<()> {
        use mongodb::IndexModel;
        use mongodb::bson::doc;

        // media: index on file.sha256 for dedup, metadata.created_at for timeline
        let media_col: Collection<MediaDoc> = db.collection("media");
        media_col.create_index(
            IndexModel::builder()
                .keys(doc! { "file.sha256": 1 })
                .build()
        ).await?;
        media_col.create_index(
            IndexModel::builder()
                .keys(doc! { "metadata.created_at": -1 })
                .build()
        ).await?;
        media_col.create_index(
            IndexModel::builder()
                .keys(doc! { "objects.class_name": 1 })
                .build()
        ).await?;
        media_col.create_index(
            IndexModel::builder()
                .keys(doc! { "faces.name": 1 })
                .build()
        ).await?;

        // vector_embeddings: index on media_id
        let vec_col: Collection<VectorEmbeddingDoc> = db.collection("vector_embeddings");
        vec_col.create_index(
            IndexModel::builder()
                .keys(doc! { "media_id": 1 })
                .build()
        ).await?;

        // person: index on face_id
        let person_col: Collection<PersonDoc> = db.collection("person");
        person_col.create_index(
            IndexModel::builder()
                .keys(doc! { "face_id": 1 })
                .build()
        ).await?;

        Ok(())
    }

    pub fn media(&self) -> Collection<MediaDoc> {
        self.db.collection("media")
    }

    pub fn person(&self) -> Collection<PersonDoc> {
        self.db.collection("person")
    }

    pub fn search_history(&self) -> Collection<SearchHistoryDoc> {
        self.db.collection("search_history")
    }

    pub fn vector_embeddings(&self) -> Collection<VectorEmbeddingDoc> {
        self.db.collection("vector_embeddings")
    }
}
