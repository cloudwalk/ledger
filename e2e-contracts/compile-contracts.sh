#!/bin/bash
#
# Compiles a subset or relevant Solidity contracts.
#
source $(dirname $0)/_functions.sh

# ------------------------------------------------------------------------------
# Functions
# ------------------------------------------------------------------------------

# Compile Solidity contracts from a project.
compile_contract() {
    repo=$1
    contract=$2
    log "Compiling: $contract ($repo)"

    # compile
    solc --base-path repos/$repo/contracts --include-path repos/$repo/node_modules --hashes --optimize -o repos/$repo/target --overwrite repos/$repo/contracts/$contract.sol

    # copy from target folder to tests
    cp repos/$repo/target/$contract.signatures ../static/contracts/
}

# ------------------------------------------------------------------------------
# Execution
# ------------------------------------------------------------------------------

# configure tools
asdf local solidity 0.8.16

# execute
compile_contract brlc-token          BRLCToken

compile_contract brlc-periphery      CardPaymentProcessor
compile_contract brlc-periphery      CashbackDistributor
compile_contract brlc-periphery      PixCashier
compile_contract brlc-periphery      TokenDistributor

compile_contract brlc-multisig       MultiSigWallet

compile_contract compound-periphery  CompoundAgent