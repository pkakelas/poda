// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {Poda} from "../src/Poda.sol";

contract PodaScript is Script {
    Poda public poda;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        poda = new Poda(msg.sender, 1000);
        console.log("Poda contract address:", address(poda));

        vm.stopBroadcast();
    }
}
