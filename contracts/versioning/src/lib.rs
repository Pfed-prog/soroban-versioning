#![no_std]

mod domain_contract {
    soroban_sdk::contractimport!(
        file = "../domain_3ebbeec072f4996958d4318656186732773ab5f0c159dcf039be202b4ecb8af8.wasm"
    );
}

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, vec, Address, Bytes,
    BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};
use soroban_sdk::{contracterror, contractmeta};

contractmeta!(key = "Description", val = "Tansu - Soroban Versioning");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractErrors {
    UnexpectedError = 0,
    InvalidKey = 1,
    ProjectAlreadyExist = 2,
    UnregisteredMaintainer = 3,
    NoHashFound = 4,
    InvalidDomainError = 5,
    MaintainerNotDomainOwner = 6,
}

#[contracttype]
pub enum DataKey {
    Admin, // Contract administrator
}

#[contracttype]
pub enum ProjectKey {
    Key(Bytes),      // UUID of the project from keccak256(name)
    LastHash(Bytes), // last hash of the project
}

#[contracttype]
pub struct Config {
    pub url: String,  // link to toml file with project metadata
    pub hash: String, // hash of the file found at the URL
}

#[contracttype]
pub struct Project {
    pub name: String,
    pub config: Config,
    pub maintainers: Vec<Address>,
}

#[contract]
pub struct Versioning;

#[contractimpl]
impl Versioning {
    pub fn init(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn version() -> u32 {
        1
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Register a new Git projects and associated metadata.
    pub fn register(
        env: Env,
        maintainer: Address,
        name: String,
        maintainers: Vec<Address>,
        url: String,
        hash: String,
        domain_contract_id: Address,
    ) -> Bytes {
        let project = Project {
            name: name.clone(),
            config: Config { url, hash },
            maintainers,
        };
        let str_len = name.len() as usize;
        if str_len > 15 {
            // could add more checks but handled in any case with later calls
            panic_with_error!(&env, &ContractErrors::InvalidDomainError);
        }
        let mut slice: [u8; 15] = [0; 15];
        name.copy_into_slice(&mut slice[..str_len]);
        let name_b = Bytes::from_slice(&env, &slice[0..str_len]);

        let key: Bytes = env.crypto().keccak256(&name_b).into();

        let key_ = ProjectKey::Key(key.clone());
        if env
            .storage()
            .persistent()
            .get::<ProjectKey, Project>(&key_)
            .is_some()
        {
            panic_with_error!(&env, &ContractErrors::ProjectAlreadyExist);
        } else {
            auth_maintainers(&env, &maintainer, &project.maintainers);

            let node = domain_node(&env, &key);
            let record_keys = domain_contract::RecordKeys::Record(node);

            let domain_client = domain_contract::Client::new(&env, &domain_contract_id);
            match domain_client.try_record(&record_keys) {
                Ok(Ok(None)) => domain_register(&env, &name_b, &maintainer, domain_contract_id),
                Ok(Ok(Some(domain_contract::Record::Domain(domain)))) => {
                    if domain.owner != maintainer {
                        panic_with_error!(&env, &ContractErrors::MaintainerNotDomainOwner)
                    }
                }
                _ => panic_with_error!(&env, &ContractErrors::InvalidDomainError),
            }
            env.storage().persistent().set(&key_, &project);

            env.events()
                .publish((symbol_short!("register"), key.clone()), name);
            key
        }
    }

    /// Change the configuration of the project.
    pub fn update_config(
        env: Env,
        maintainer: Address,
        key: Bytes,
        maintainers: Vec<Address>,
        url: String,
        hash: String,
    ) {
        let key_ = ProjectKey::Key(key);
        if let Some(mut project) = env.storage().persistent().get::<ProjectKey, Project>(&key_) {
            auth_maintainers(&env, &maintainer, &project.maintainers);

            let config = Config { url, hash };
            project.config = config;
            project.maintainers = maintainers;
            env.storage().persistent().set(&key_, &project);
        } else {
            panic_with_error!(&env, &ContractErrors::InvalidKey);
        }
    }

    /// Set the last commit hash
    pub fn commit(env: Env, maintainer: Address, project_key: Bytes, hash: String) {
        let key_ = ProjectKey::Key(project_key.clone());
        if let Some(project) = env.storage().persistent().get::<ProjectKey, Project>(&key_) {
            auth_maintainers(&env, &maintainer, &project.maintainers);
            env.storage()
                .persistent()
                .set(&ProjectKey::LastHash(project_key.clone()), &hash);
            env.events()
                .publish((symbol_short!("commit"), project_key), hash);
        } else {
            panic_with_error!(&env, &ContractErrors::InvalidKey);
        }
    }

    /// Get the last commit hash
    pub fn get_commit(env: Env, project_key: Bytes) -> String {
        let key_ = ProjectKey::Key(project_key.clone());
        if env
            .storage()
            .persistent()
            .get::<ProjectKey, Project>(&key_)
            .is_some()
        {
            env.storage()
                .persistent()
                .get(&ProjectKey::LastHash(project_key))
                .unwrap_or_else(|| {
                    panic_with_error!(&env, &ContractErrors::NoHashFound);
                })
        } else {
            panic_with_error!(&env, &ContractErrors::InvalidKey);
        }
    }
}

fn auth_maintainers(env: &Env, maintainer: &Address, maintainers: &Vec<Address>) {
    maintainer.require_auth();
    if !maintainers.contains(maintainer) {
        panic_with_error!(&env, &ContractErrors::UnregisteredMaintainer);
    }
}

/// Register a Soroban Domain: https://sorobandomains.org
fn domain_register(env: &Env, name: &Bytes, maintainer: &Address, domain_contract_id: Address) {
    let tld = Bytes::from_slice(env, &[120, 108, 109]); // xlm
    let min_duration: u64 = 31536000;

    // Convert the arguments to Val
    let name_raw = name.to_val();
    let tld_raw = tld.to_val();
    let maintainer_raw = maintainer.to_val();
    let min_duration_raw: Val = min_duration.into_val(env);

    // Construct the init_args
    let init_args = vec![
        &env,
        name_raw,
        tld_raw,
        maintainer_raw,
        maintainer_raw,
        min_duration_raw,
    ];

    env.invoke_contract::<()>(
        &domain_contract_id,
        &Symbol::new(env, "set_record"),
        init_args,
    );
}

fn domain_node(env: &Env, domain: &Bytes) -> BytesN<32> {
    let tld = Bytes::from_slice(env, &[120, 108, 109]); // xlm
    let parent_hash: Bytes = env.crypto().keccak256(&tld).into();
    let mut node_builder: Bytes = Bytes::new(env);
    node_builder.append(&parent_hash);
    node_builder.append(domain);

    env.crypto().keccak256(&node_builder).into()
}

mod test;
