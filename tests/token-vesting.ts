import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { TokenVesting } from "../target/types/token_vesting";

describe("token-vesting", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());

  const program = anchor.workspace.TokenVesting as Program<TokenVesting>;

  it("Is initialized!", async () => {
    // Add your test here.
    const tx = await program.rpc.initialize({});
    console.log("Your transaction signature", tx);
  });
});
