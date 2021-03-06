// Copyright 2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{Client, Error, Result, Seed};

use bee_message::prelude::{Address, Bech32Address, Ed25519Address};
use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use core::convert::TryInto;
use slip10::BIP32Path;
use std::ops::Range;

const HARDENED: u32 = 1 << 31;

/// Builder of get_addresses API
pub struct GetAddressesBuilder<'a> {
    client: Option<&'a Client>,
    seed: Option<&'a Seed>,
    account_index: usize,
    range: Range<usize>,
    bech32_hrp: Option<String>,
}

impl<'a> Default for GetAddressesBuilder<'a> {
    fn default() -> Self {
        Self {
            client: None,
            seed: None,
            account_index: 0,
            range: 0..20,
            bech32_hrp: None,
        }
    }
}

impl<'a> GetAddressesBuilder<'a> {
    /// Create get_addresses builder
    pub fn new(seed: &'a Seed) -> Self {
        Self {
            seed: Some(seed),
            ..Default::default()
        }
    }

    /// Provide a client to get the bech32_hrp from the node
    pub fn with_client(mut self, client: &'a Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Set the account index
    pub fn with_account_index(mut self, account_index: usize) -> Self {
        self.account_index = account_index;
        self
    }

    /// Set range to the builder
    pub fn with_range(mut self, range: Range<usize>) -> Self {
        self.range = range;
        self
    }

    /// Set bech32 human readable part (hrp)
    pub fn with_bech32_hrp(mut self, bech32_hrp: String) -> Self {
        self.bech32_hrp = Some(bech32_hrp);
        self
    }

    /// Consume the builder and get a vector of public Bech32Addresses
    pub async fn finish(self) -> Result<Vec<Bech32Address>> {
        Ok(self
            .get_all()
            .await?
            .into_iter()
            .filter(|(_, internal)| !internal)
            .map(|(a, _)| a)
            .collect::<Vec<Bech32Address>>())
    }

    /// Consume the builder and get the vector of Bech32Addresses
    pub async fn get_all(self) -> Result<Vec<(Bech32Address, bool)>> {
        let mut path = BIP32Path::from_str(&crate::account_path!(self.account_index)).expect("invalid account index");

        let mut addresses = Vec::new();
        let bech32_hrp = match self.bech32_hrp {
            Some(bech32_hrp) => bech32_hrp,
            None => {
                self.client
                    .ok_or_else(|| Error::MissingParameter(String::from("Client or bech32_hrp")))?
                    .get_bech32_hrp()
                    .await?
            }
        };
        for i in self.range {
            let address = generate_address(&self.seed.unwrap(), &mut path, i, false)?;
            let internal_address = generate_address(&self.seed.unwrap(), &mut path, i, true)?;
            addresses.push((Bech32Address(address.to_bech32(&bech32_hrp)), false));
            addresses.push((Bech32Address(internal_address.to_bech32(&bech32_hrp)), true));
        }

        Ok(addresses)
    }
}

fn generate_address(seed: &Seed, path: &mut BIP32Path, index: usize, internal: bool) -> Result<Address> {
    path.push(internal as u32 + HARDENED);
    path.push(index as u32 + HARDENED);

    let public_key = seed.generate_private_key(path)?.public_key().to_compressed_bytes();
    // Hash the public key to get the address
    let mut hasher = VarBlake2b::new(32).unwrap();
    hasher.update(public_key);
    let mut result: [u8; 32] = [0; 32];
    hasher.finalize_variable(|res| {
        result = res.try_into().expect("Invalid Length of Public Key");
    });

    path.pop();
    path.pop();

    Ok(Address::Ed25519(Ed25519Address::new(result)))
}

/// Function to find the index and public or internal type of an Bech32 encoded address
pub async fn search_address(
    seed: &Seed,
    bech32_hrp: String,
    account_index: usize,
    range: Range<usize>,
    address: &Bech32Address,
) -> Result<(usize, bool)> {
    let addresses = GetAddressesBuilder::new(&seed)
        .with_bech32_hrp(bech32_hrp)
        .with_account_index(account_index)
        .with_range(range.clone())
        .get_all()
        .await?;
    let mut index_counter = 0;
    for address_internal in addresses {
        if address_internal.0 == *address {
            return Ok((index_counter, address_internal.1));
        }
        if !address_internal.1 {
            index_counter += 1;
        }
    }
    Err(crate::error::Error::InputAddressNotFound(format!("{:?}", range)))
}
