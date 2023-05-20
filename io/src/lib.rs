#![no_std]

use gmeta::{In, InOut, Metadata};
use gstd::{ops::Range, prelude::*, ActorId, MessageId};

pub type Checksum = u32;

pub struct FungibleTokenMetadata;

impl Metadata for FungibleTokenMetadata {
    type Init = In<Initialize>;
    type Handle = InOut<FTAction, FTEvent>;
    type Others = ();
    type Reply = ();
    type Signal = ();
    type State = IoFungibleToken;
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum Initialize {
    Config(InitConfig),
    State(IoFungibleToken),
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct InitConfig {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum FTAction {
    Mint(u128),
    Burn(u128),
    Transfer {
        from: ActorId,
        to: ActorId,
        amount: u128,
    },
    Approve {
        to: ActorId,
        amount: u128,
    },
    TotalSupply,
    BalanceOf(ActorId),
    UpgradeState {
        data: Vec<u8>,
        total_checksum: Checksum,
        range: gstd::ops::Range<u64>,
        part: Part,
    },
    MigrateState(ActorId),
}

#[derive(Debug, Decode, Encode, TypeInfo, Clone, Copy)]
pub struct Part {
    pub current: u64,
    pub total: u64,
}

#[derive(Debug, Encode, Decode, TypeInfo)]
pub enum FTEvent {
    Transfer {
        from: ActorId,
        to: ActorId,
        amount: u128,
    },
    Approve {
        from: ActorId,
        to: ActorId,
        amount: u128,
    },
    TotalSupply(u128),
    Balance(u128),
    StateUpgraded {
        checksum: Checksum,
    },
    StateMigrated {
        checksum: Checksum,
    },
    AlreadyInMigrationState,
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum MigrationRole {
    Sender { migrate_to: ActorId },
    Receiver { migration_initiator: ActorId },
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct MigrationState {
    pub migration_role: MigrationRole,
    pub data: Vec<u8>,
    pub part: Part,
    pub ranges: Vec<Range<u64>>,
    pub total_checksum: Checksum,
    pub message_id: MessageId,
}

#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo)]
pub struct IoFungibleToken {
    pub name: String,
    pub symbol: String,
    pub total_supply: u128,
    pub balances: Vec<(ActorId, u128)>,
    pub allowances: Vec<(ActorId, Vec<(ActorId, u128)>)>,
    pub decimals: u8,
    pub migration_state: Option<MigrationState>,
}

impl gstd::fmt::Display for MigrationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let role = match self.migration_role {
            MigrationRole::Sender { .. } => "Sender",
            MigrationRole::Receiver { .. } => "Receiver",
        };

        write!(
            f,
            "Migration State: role: {role}, {}/{}, {:?}",
            self.part.current, self.part.total, self.message_id
        )
    }
}
