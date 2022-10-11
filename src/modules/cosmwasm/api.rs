use std::str::FromStr;

use cosmrs::tx::Msg;
use serde::Serialize;

use cosmos_sdk_proto::cosmwasm::wasm::v1::{
    AccessConfig, QuerySmartContractStateRequest, QuerySmartContractStateResponse,
}; // TODO: make my own type in chain::model
use cosmrs::cosmwasm::{MsgExecuteContract, MsgInstantiateContract, MsgStoreCode};
use cosmrs::tendermint::abci::tag::Key;
use cosmrs::tendermint::abci::Event;

use crate::chain::error::ChainError;
use crate::chain::tx::sign_tx;
use crate::clients::client::CosmTome;

use crate::{chain::model::Coin, clients::client::CosmosClient, key::key::SigningKey};

use super::model::{ExecResponse, QueryResponse};
use super::{
    error::CosmwasmError,
    model::{InstantiateResponse, StoreCodeResponse},
};

// TODO: Dont unwrap in this file

pub struct Cosmwasm {}

impl Cosmwasm {
    pub(crate) async fn wasm_store<T: CosmosClient>(
        &self,
        client: &CosmTome<T>,
        payload: Vec<u8>,
        key: &SigningKey,
        instantiate_perms: Option<AccessConfig>,
        simulate: bool,
    ) -> Result<StoreCodeResponse, CosmwasmError> {
        let account_id = key.to_account(&client.cfg.prefix)?;

        let msg = MsgStoreCode {
            sender: account_id.clone(),
            wasm_byte_code: payload,
            instantiate_permission: instantiate_perms
                .map(|p| p.try_into())
                .transpose()
                .map_err(|e| CosmwasmError::InstantiatePerms { source: e })?,
        }
        .to_any()
        .map_err(ChainError::proto_encoding)?;

        let tx_raw = sign_tx(client, msg, key, None, account_id.to_string()).await?;

        let res = client.client.broadcast_tx(&tx_raw).await?;

        let code_id = find_event(&res.events, "store_code")
            .unwrap() // TODO: Dont unwrap
            .attributes
            .iter()
            .find(|a| a.key == Key::from_str("code_id").unwrap())
            .unwrap()
            .value
            .as_ref()
            .parse::<u64>()
            .unwrap();

        Ok(StoreCodeResponse { code_id, res: res })
    }

    pub(crate) async fn wasm_instantiate<S: Serialize, T: CosmosClient>(
        &self,
        client: &CosmTome<T>,
        code_id: u64,
        msg: &S,
        key: &SigningKey,
        admin: Option<String>,
        funds: Vec<Coin>,
        simulate: bool,
    ) -> Result<InstantiateResponse, CosmwasmError> {
        let payload = serde_json::to_vec(msg).map_err(CosmwasmError::json)?;
        let account_id = key.to_account(&client.cfg.prefix)?;

        let mut cosm_funds = vec![];
        for fund in funds {
            cosm_funds.push(fund.try_into()?);
        }

        let msg = MsgInstantiateContract {
            sender: account_id.clone(),
            admin: admin
                .map(|s| s.parse())
                .transpose()
                .map_err(|_| CosmwasmError::AdminAddress)?,
            code_id,
            label: Some("cosm-tome".to_string()), // TODO: Dont hardcode
            msg: payload,
            funds: cosm_funds,
        }
        .to_any()
        .map_err(ChainError::proto_encoding)?;

        let tx_raw = sign_tx(client, msg, key, None, account_id.to_string()).await?;

        let res = client.client.broadcast_tx(&tx_raw).await?;

        // TODO: Make a better more generic unified response type from all of the clients instead of just making all clients conform to this tendermint Events meme
        let addr = find_event(&res.events, "instantiate")
            .unwrap() // TODO: Dont unwrap
            .attributes
            .iter()
            .find(|a| a.key == Key::from_str("_contract_address").unwrap())
            .unwrap()
            .value
            .to_string();

        Ok(InstantiateResponse {
            address: addr,
            res: res,
        })
    }

    pub(crate) async fn wasm_execute<S: Serialize, T: CosmosClient>(
        &self,
        client: &CosmTome<T>,
        address: String,
        msg: &S,
        key: &SigningKey,
        funds: Vec<Coin>,
        simulate: bool,
    ) -> Result<ExecResponse, CosmwasmError> {
        let payload = serde_json::to_vec(msg).map_err(CosmwasmError::json)?;

        let account_id = key.to_account(&client.cfg.prefix)?;

        let mut cosm_funds = vec![];
        for fund in funds {
            cosm_funds.push(fund.try_into()?);
        }

        let msg = MsgExecuteContract {
            sender: account_id.clone(),
            contract: address.parse().unwrap(),
            msg: payload,
            funds: cosm_funds,
        }
        .to_any()
        .map_err(ChainError::proto_encoding)?;

        let tx_raw = sign_tx(client, msg, key, None, account_id.to_string()).await?;

        let res = client.client.broadcast_tx(&tx_raw).await?;

        Ok(ExecResponse { res })
    }

    pub(crate) async fn wasm_query<S: Serialize, T: CosmosClient>(
        &self,
        client: &CosmTome<T>,
        address: String,
        msg: &S,
    ) -> Result<QueryResponse, CosmwasmError> {
        let payload = serde_json::to_vec(msg).map_err(CosmwasmError::json)?;

        let req = QuerySmartContractStateRequest {
            address: address.parse().unwrap(),
            query_data: payload,
        };

        let res = client
            .client
            .query::<_, QuerySmartContractStateRequest, QuerySmartContractStateResponse>(
                req,
                "/cosmwasm.wasm.v1.Query/SmartContractState",
            )
            .await?;

        Ok(QueryResponse { res: res.into() })
    }

    // TODO: Finish
    // pub async fn migrate(
    //     &self,
    //     address: String,
    //     new_code_id: u64,
    //     payload: Vec<u8>,
    //     key: &SigningKey,
    //     simulate: bool,
    // ) -> Result<MigrateResponse, ClientError> {
    //     todo!()
    // }
}

//  TODO: Make this more useful, and make this a method on the custom Event
pub fn find_event(events: &Vec<Event>, key_name: &str) -> Option<Event> {
    for event in events {
        if event.type_str == key_name {
            return Some(event.clone());
        }
    }
    None
}