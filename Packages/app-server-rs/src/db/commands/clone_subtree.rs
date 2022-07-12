use indexmap::IndexMap;
use itertools::Itertools;
use jsonschema::JSONSchema;
use jsonschema::output::BasicOutput;
use lazy_static::lazy_static;
use anyhow::{anyhow, Context, Error};
use async_graphql::{Object, Schema, Subscription, ID, async_stream, OutputType, scalar, EmptySubscription, SimpleObject};
use deadpool_postgres::{Pool, Client, Transaction};
use futures_util::{Stream, stream, TryFutureExt, StreamExt, Future, TryStreamExt};
use hyper::{Body, Method};
use rust_macros::wrap_slow_macros;
use serde::{Serialize, Deserialize};
use serde_json::json;
use tokio::sync::RwLock;
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;
use tracing::info;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::{time::Duration, pin::Pin, task::Poll};

use crate::db::_general::GenericMutation_Result;
use crate::db::access_policies::AccessPolicy;
use crate::db::general::subtree::get_subtree;
use crate::db::medias::Media;
use crate::db::node_child_links::{NodeChildLink, get_node_child_links};
use crate::db::node_phrasings::MapNodePhrasing;
use crate::db::node_revisions::MapNodeRevision;
use crate::db::node_tags::MapNodeTag;
use crate::db::nodes::MapNode;
use crate::db::terms::Term;
use crate::links::proxy_to_asjs::{HyperClient, APP_SERVER_JS_URL};
use crate::utils::db::accessors::AccessorContext;
use crate::utils::db::filter::{QueryFilter, FilterInput};
use crate::utils::db::pg_stream_parsing::RowData;
use crate::utils::db::sql_fragment::SQLFragment;
use crate::utils::db::sql_param::SQLIdent;
use crate::utils::db::transactions::{start_read_transaction, start_write_transaction};
use crate::utils::db::uuid::new_uuid_v4_as_b64;
use crate::utils::general::data_anchor::{DataAnchorFor1, DataAnchor};
use crate::utils::general::general::{to_anyhow, to_anyhow_with_extra};
use crate::utils::{db::{handlers::{handle_generic_gql_collection_request, handle_generic_gql_doc_request, GQLSet}}};
use crate::utils::type_aliases::{JSONValue, PGClientObject};

use super::_command::{set_db_entry_by_id, set_db_entry_by_id_for_struct};

//wrap_slow_macros!{}

#[derive(Serialize, Deserialize, Debug)]
pub struct CloneSubtreePayload {
    parentNodeID: String,
    rootNodeID: String,
    maxDepth: Option<usize>,
}
lazy_static! {
    static ref CLONE_SUBTREE_PAYLOAD_SCHEMA_JSON: JSONValue = json!({
        "properties": {
            "parentNodeID": {"type": "string"},
            "rootNodeID": {"type": "string"},
            "maxDepth": {"type": "number"},
        },
        "required": ["parentNodeID", "rootNodeID", "maxDepth"],
    });
    static ref CLONE_SUBTREE_PAYLOAD_SCHEMA_JSON_COMPILED: JSONSchema = JSONSchema::compile(&CLONE_SUBTREE_PAYLOAD_SCHEMA_JSON).expect("A valid schema");
}

pub async fn clone_subtree(gql_ctx: &async_graphql::Context<'_>, payload_raw: JSONValue) -> Result<GenericMutation_Result, Error> {
    let output: BasicOutput = CLONE_SUBTREE_PAYLOAD_SCHEMA_JSON_COMPILED.apply(&payload_raw).basic();
    if !output.is_valid() {
        let output_json = serde_json::to_value(output)?;
        return Err(anyhow!(output_json));
    }
    let payload: CloneSubtreePayload = serde_json::from_value(payload_raw)?;
    
    let mut anchor = DataAnchorFor1::empty(); // holds pg-client
    let tx = start_write_transaction(&mut anchor, gql_ctx).await?;
    let ctx = AccessorContext::new(tx);

    // probably temp: helper for logging
    let log = |text: &str| {
        info!("CloneSubtreeLog: {text}");
        //msg_sender.send(GeneralMessage::MigrateLogMessageAdded(text.to_owned())).unwrap();
    };

    log("part 0");
    let subtree = get_subtree(&ctx, payload.rootNodeID.clone(), payload.maxDepth).await?;
    // these don't need cloning (since they don't "reference back"): terms, medias

    log("part 0.5");
    let ids = subtree.get_all_ids();
    let mut id_replacements: IndexMap<String, String> = IndexMap::new();
    for id in &ids {
        id_replacements.insert(id.clone(), new_uuid_v4_as_b64());
    }
    let get_new_id_str = |old_id: &String| id_replacements.get(old_id).unwrap().to_owned();
    let get_new_id = |old_id: &ID| ID(get_new_id_str(&old_id.to_string()));

    log("part 0.75");
	// defer database's checking of foreign-key constraints until the end of the transaction (else would error)
    ctx.tx.execute("SET CONSTRAINTS ALL DEFERRED;", &[]).await?;

    log("part 1");
    // first, add a new link from the old-node's parent to the new-node (which we've generated an id for, and are about to construct)
    let old_root_links = get_node_child_links(&ctx, Some(payload.parentNodeID.as_str()), Some(payload.rootNodeID.as_str())).await?;
    let old_root_link = old_root_links.get(0).ok_or(anyhow!("No child-link found between provided root-node \"{}\" and parent \"{}\".", payload.rootNodeID, payload.parentNodeID))?;
    let mut new_root_link = old_root_link.clone();
    new_root_link.child = id_replacements.get(&payload.rootNodeID).ok_or(anyhow!("Generation of new id for clone of root-node failed somehow."))?.to_owned();
    log("part 1.5");
    set_db_entry_by_id_for_struct(&ctx, "nodeChildLinks".to_owned(), new_uuid_v4_as_b64(), new_root_link).await?;

    log("part 2");
    for node_old in subtree.nodes {
        let mut node = node_old.clone();
        node.id = get_new_id(&node.id);
        node.c_currentRevision = get_new_id_str(&node.c_currentRevision);
        set_db_entry_by_id_for_struct(&ctx, "nodes".to_owned(), node.id.to_string(), node).await?;
    }
    log("part 3");
    for rev_old in subtree.nodeRevisions {
        let mut rev = rev_old.clone();
        rev.id = get_new_id(&rev.id);
        /*for attachment in &rev.attachments {
            if let Some(media) = attachment.media {
            }
        }*/
        rev.node = get_new_id_str(&rev.node);
        set_db_entry_by_id_for_struct(&ctx, "nodeRevisions".to_owned(), rev.id.to_string(), rev).await?;
    }
    log("part 4");
    for phrasing_old in subtree.nodePhrasings {
        let mut phrasing = phrasing_old.clone();
        phrasing.id = get_new_id(&phrasing.id);
        phrasing.node = get_new_id_str(&phrasing.node);
        set_db_entry_by_id_for_struct(&ctx, "nodePhrasings".to_owned(), phrasing.id.to_string(), phrasing).await?;
    }
    log("part 5");
    for link_old in subtree.nodeChildLinks {
        let mut link = link_old.clone();
        link.id = get_new_id(&link.id);
        link.parent = get_new_id_str(&link.parent);
        link.child = get_new_id_str(&link.child);
        set_db_entry_by_id_for_struct(&ctx, "nodeChildLinks".to_owned(), link.id.to_string(), link).await?;
    }
    log("part 6");
    for tag_old in subtree.nodeTags {
        let mut tag = tag_old.clone();
        tag.id = get_new_id(&tag.id);

        // todo: recreate the CalculateNodeIDsForTag function, so we don't need manual updating of the "nodes" field

        // for now, only transfer tags in the "basics" group like labels, and special-cases like clone-history tags (see MaybeCloneAndRetargetNodeTag func)
        if let Some(mut labels) = tag.labels.as_mut() {
            let old_nodeX_id = labels.nodeX.clone();
            labels.nodeX = get_new_id_str(&labels.nodeX);
            tag.nodes = tag.nodes.into_iter().map(|node_id| if node_id == old_nodeX_id { labels.nodeX.clone() } else { node_id }).collect_vec();
        }
        // clone-history tags are a special case: clone+extend them if-and-only-if the result/final-entry is the old-node (preserving history without creating confusion)
        if let Some(clone_history) = tag.cloneHistory.as_mut() {
            let last_node_in_history = clone_history.cloneChain.last();
            if let Some(last_node_in_history) = last_node_in_history && ids.contains(last_node_in_history) {
                let new_node_id = id_replacements.get(last_node_in_history).unwrap().to_owned();
                clone_history.cloneChain.push(new_node_id.clone());
                tag.nodes.push(new_node_id);
            } else {
                // this tag marks the old-node merely as the source for a clone, which shouldn't transfer to new node, so skip this tag (ie. don't clone it)
                continue;
            }
        }

        set_db_entry_by_id_for_struct(&ctx, "nodeTags".to_owned(), tag.id.to_string(), tag).await?;
    }

    log("Committing transaction...");
    ctx.tx.commit().await?;
    log("Clone-subtree command complete!");
    Ok(GenericMutation_Result {
        message: "Command completed successfully.".to_owned(),
    })
}