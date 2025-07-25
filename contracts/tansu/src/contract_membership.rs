use soroban_sdk::{Address, Bytes, Env, String, Vec, contractimpl, panic_with_error};

use crate::{MembershipTrait, Tansu, TansuArgs, TansuClient, errors, types};

#[contractimpl]
impl MembershipTrait for Tansu {
    fn add_member(env: Env, member_address: Address, meta: String) {
        member_address.require_auth();

        let member_key_ = types::DataKey::Member(member_address.clone());
        if env
            .storage()
            .persistent()
            .get::<types::DataKey, types::Member>(&member_key_)
            .is_some()
        {
            panic_with_error!(&env, &errors::ContractErrors::MemberAlreadyExist)
        } else {
            let member = types::Member {
                projects: Vec::new(&env),
                meta,
            };
            env.storage().persistent().set(&member_key_, &member);
        };
    }

    fn get_member(env: Env, member_address: Address) -> types::Member {
        let member_key_ = types::DataKey::Member(member_address.clone());
        env.storage()
            .persistent()
            .get::<types::DataKey, types::Member>(&member_key_)
            .unwrap_or_else(|| {
                panic_with_error!(&env, &errors::ContractErrors::UnknownMember);
            })
    }

    fn add_badges(
        env: Env,
        maintainer: Address,
        key: Bytes,
        member: Address,
        badges: Vec<types::Badge>,
    ) {
        let project_key_ = types::ProjectKey::Key(key.clone());
        if let Some(project) = env
            .storage()
            .persistent()
            .get::<types::ProjectKey, types::Project>(&project_key_)
        {
            crate::auth_maintainers(&env, &maintainer, &project.maintainers);
        } else {
            panic_with_error!(&env, &errors::ContractErrors::InvalidKey)
        };

        let member_key_ = types::DataKey::Member(member.clone());
        let mut member_ = if let Some(member_) = env
            .storage()
            .persistent()
            .get::<types::DataKey, types::Member>(&member_key_)
        {
            member_
        } else {
            panic_with_error!(&env, &errors::ContractErrors::UnknownMember)
        };

        'member_projects_badges: {
            for mut member_project_ in member_.projects.iter() {
                if member_project_.project == key {
                    for badge_ in badges.iter() {
                        if !member_project_.badges.contains(badge_.clone()) {
                            member_project_.badges.push_back(badge_);
                        }
                    }
                    break 'member_projects_badges;
                }
            }
            let project_badges = types::ProjectBadges {
                project: key.clone(),
                badges: badges.clone(),
            };
            member_.projects.push_back(project_badges);
        }

        let badges_key_ = types::ProjectKey::Badges(key.clone());
        let mut badges_ = <Tansu as MembershipTrait>::get_badges(env.clone(), key);

        for badge_kind in [
            types::Badge::Developer,
            types::Badge::Triage,
            types::Badge::Community,
        ] {
            // Pick the right vector for this badge kind
            let vec_ref: &mut Vec<Address> = match badge_kind {
                types::Badge::Developer => &mut badges_.developer,
                types::Badge::Triage => &mut badges_.triage,
                types::Badge::Community => &mut badges_.community,
                _ => continue,
            };

            // Build a cleaned-up copy removing all badges from member
            let mut new_vec: Vec<Address> = Vec::new(&env);
            for addr in vec_ref.iter() {
                if addr != member.clone() {
                    new_vec.push_back(addr);
                }
            }
            // Add the member back if they should hold this badge now
            if badges.contains(badge_kind.clone()) {
                new_vec.push_back(member.clone());
            }
            // Replace the old vector
            *vec_ref = new_vec;
        }

        env.storage().persistent().set(&badges_key_, &badges_);
        env.storage().persistent().set(&member_key_, &member_);
    }

    fn get_badges(env: Env, key: Bytes) -> types::Badges {
        let badges_key_ = types::ProjectKey::Badges(key);
        if let Some(badges_) = env
            .storage()
            .persistent()
            .get::<types::ProjectKey, types::Badges>(&badges_key_)
        {
            badges_
        } else {
            types::Badges {
                developer: Vec::new(&env),
                triage: Vec::new(&env),
                community: Vec::new(&env),
                verified: Vec::new(&env),
                default: Vec::new(&env),
            }
        }
    }

    fn get_max_weight(env: Env, project_key: Bytes, member_address: Address) -> u32 {
        let member_key = types::DataKey::Member(member_address.clone());

        if let Some(member) = env
            .storage()
            .persistent()
            .get::<types::DataKey, types::Member>(&member_key)
        {
            match member
                .projects
                .iter()
                .find(|project_badges| project_badges.project == project_key)
            {
                Some(project_badges) => {
                    if project_badges.badges.is_empty() {
                        types::Badge::Default as u32
                    } else {
                        project_badges
                            .badges
                            .iter()
                            .map(|badge| badge as u32)
                            .sum::<u32>()
                    }
                }
                _ => types::Badge::Default as u32,
            }
        } else {
            types::Badge::Default as u32
        }
    }
}
