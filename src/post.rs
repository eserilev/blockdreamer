use eth2::types::{BeaconBlock, EthSpec, Slot};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{create_dir_all, File};
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct PostEndpoint {
    client: Client,
    url: String,
    persistence_dir: Option<PathBuf>,
    compare_rewards: bool,
}

impl PostEndpoint {
    pub fn new(url: String, persistence_dir: Option<PathBuf>, compare_rewards: bool) -> Arc<Self> {
        let client = Client::new();
        Arc::new(Self {
            client,
            url,
            persistence_dir,
            compare_rewards,
        })
    }

    pub async fn post_blocks<E: EthSpec>(
        &self,
        names_and_labels: Vec<(String, String)>,
        blocks: Vec<BeaconBlock<E>>,
        slot: Slot,
    ) -> Result<(), String> {
        let response = self
            .client
            .post(&self.url)
            .json(&blocks)
            .send()
            .await
            .map_err(|e| format!("POST error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "POST failed: {}",
                response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<body garbled>".into())
            ));
        }

        let response_json: Vec<Value> = response
            .json()
            .await
            .map_err(|e| format!("invalid JSON from POST endpoint: {}", e))?;

        let mut max_reward = 0;
        let mut max_reward_nodes = vec![];

        for ((name, label), result) in names_and_labels.into_iter().zip(response_json) {
            if self.compare_rewards {
                let reward = result["attestation_rewards"]["total"].as_u64().unwrap();
                println!("reward from {name}: {reward} gwei");

                if reward > max_reward {
                    max_reward = reward;
                    max_reward_nodes = vec![name.clone()];
                } else if reward == max_reward {
                    max_reward_nodes.push(name.clone());
                }
            }

            if let Some(persistence_dir) = &self.persistence_dir {
                // Store results by client label (same format as blockprint training data).
                let label_dir = persistence_dir.join(label);
                create_dir_all(&label_dir)
                    .await
                    .map_err(|e| format!("unable to create {}: {}", label_dir.display(), e))?;

                // Name files by node name and slot.
                let result_path = label_dir.join(format!("{name}_{slot}.json"));
                let mut f = File::create(&result_path)
                    .await
                    .map_err(|e| format!("unable to create {}: {}", result_path.display(), e))?;

                let bytes =
                    serde_json::to_vec(&result).map_err(|e| format!("JSON error: {}", e))?;
                f.write_all(&bytes)
                    .await
                    .map_err(|e| format!("unable to write {}: {}", result_path.display(), e))?;
            }
        }

        if self.compare_rewards {
            println!("most profitable block from {max_reward_nodes:?}");
        }

        Ok(())
    }
}
