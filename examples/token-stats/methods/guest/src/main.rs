// Copyright 2024 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloy_primitives::B256;
use alloy_sol_types::SolValue;
use core::{CometMainInterface, Input, Journal, CONTRACT};
use risc0_steel::{config::ETH_MAINNET_CHAIN_SPEC, Contract};
use risc0_zkvm::{guest::env, sha::Digest};

const SECONDS_PER_YEAR: u64 = 60 * 60 * 24 * 365;

fn main() {
    // Read the input from the guest environment.
    let Input {
        input,
        self_image_id,
        assumption,
    } = env::read();

    // Proof composition: recursively verify and load the previous APR computation
    let mut stats = if let Some(journal) = assumption {
        // verify that the journal was indeed produced by the same guest code
        env::verify(self_image_id, &journal).unwrap();
        // decode the journal
        let Journal {
            commitment,
            stats,
            selfImageID,
        } = Journal::abi_decode(&journal, false).unwrap();
        // check that the image ID is correct and that the input chain connects
        assert_eq!(Digest::from_bytes(selfImageID.0), self_image_id);
        input.link(&commitment);

        stats
    } else {
        Default::default()
    };

    // Converts the input into a `EvmEnv` for execution. The `with_chain_spec` method is used
    // to specify the chain configuration. It checks that the state matches the state root in the
    // header provided in the input.
    let env = input.into_env().with_chain_spec(&ETH_MAINNET_CHAIN_SPEC);

    // Execute two separate view calls.
    let contract = Contract::new(CONTRACT, &env);
    let utilization = contract
        .call_builder(&CometMainInterface::getUtilizationCall {})
        .call()
        ._0;
    let supply_rate = contract
        .call_builder(&CometMainInterface::getSupplyRateCall { utilization })
        .call()
        ._0;

    // The formula for APR in percentage is the following:
    // Seconds Per Year = 60 * 60 * 24 * 365
    // Utilization = getUtilization()
    // Supply Rate = getSupplyRate(Utilization)
    // Supply APR = Supply Rate / (10 ^ 18) * Seconds Per Year * 100
    let annual_supply_rate = supply_rate * SECONDS_PER_YEAR;
    stats.add_supply_rate(annual_supply_rate);

    // Commit to the EVM state, token stats and the image ID used for the verification.
    let journal = Journal {
        commitment: env.into_commitment(),
        stats,
        selfImageID: B256::from_slice(self_image_id.as_bytes()),
    };
    env::commit_slice(&journal.abi_encode());
}
