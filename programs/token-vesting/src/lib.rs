use std::borrow::BorrowMut;
use anchor_lang::prelude::*;
use anchor_spl::token::{TokenAccount, Transfer, Token, transfer};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod token_vesting {
    use super::*;

    /// Initializes an empty program account for the token_vesting program
    ///
    /// # Arguments
    /// * `seeds` - The seed used to derive the vesting accounts address
    /// * `number_of_schedules` - The number of release schedules for this contract to hold
    pub fn init(ctx: Context<Initialize>, seeds: [u8; 31], number_of_schedules: u32) -> Result<()> {
        let vesting = &mut ctx.accounts.vesting;
        vesting.is_initialized = false;
        vesting.schedule = vec![Schedule{release_time: 0, amount: 0}; number_of_schedules as usize];
        Ok(())
    }

    /// Creates a new vesting schedule contract
    pub fn create(ctx: Context<Create>,
                  seeds: [u8; 31],
                  mint_address: Pubkey,
                  destination_token_address: Pubkey,
                  schedules: Vec<Schedule>) -> Result<()> {

        let total_amount = total_amount(&schedules)?;
        require!(ctx.accounts.source_token.amount > total_amount, VestingError::InsufficientFunds);

        let vesting = &mut ctx.accounts.vesting;
        vesting.destination_address = destination_token_address;
        vesting.mint_address = mint_address;
        vesting.is_initialized = true;
        vesting.schedule = schedules;

        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.source_token.to_account_info(),
                to: ctx.accounts.vesting_token.to_account_info(),
                authority: ctx.accounts.source_authority.to_account_info(),
            });
        transfer(transfer_ctx, total_amount)
    }


    pub fn unlock(ctx: Context<Unlock>, seeds: [u8; 31]) -> Result<()> {
        let now = anchor_lang::solana_program::clock::Clock::get()?.unix_timestamp;
        let total_amount_to_transfer = total_amount_to_transfer(&ctx.accounts.vesting.schedule, now);

        require!(total_amount_to_transfer > 0, VestingError::ReleaseTimeNotYetReached);

        let bump = *ctx.bumps.get("vesting").unwrap();
        let seeds = &[
            seeds.as_ref(),
            &[bump],
        ];
        let signer = &[&seeds[..]];

        // Unlocks a simple vesting contract (SVC)
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vesting_token.to_account_info(),
                to: ctx.accounts.destination_token.to_account_info(),
                authority: ctx.accounts.vesting.to_account_info(),
            },
            signer
        );
        transfer(transfer_ctx, total_amount_to_transfer)?;

        // Reset released amounts to 0. This makes the simple unlock safe with complex scheduling contracts
        reset_released_amount(&mut ctx.accounts.vesting.schedule, now);

        Ok(())
    }

    /// Change the destination account of a given simple vesting contract (SVC)
    pub fn change_destination(ctx: Context<ChangeDestination>, seeds: [u8; 31]) -> Result<()> {
        let destination = &mut ctx.accounts.vesting.destination_address;
        *destination = ctx.accounts.new_destination_token.key();
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(seeds: [u8; 31], number_of_schedules: u32)]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = calc_vesting_account_size(number_of_schedules), seeds = [seeds.as_ref()], bump)]
    pub vesting: Account<'info, Vesting>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(seeds: [u8; 31], mint_address: Pubkey, destination_token_address: Pubkey, schedules: Vec<Schedule>)]
pub struct Create<'info> {
    #[account(mut, seeds = [seeds.as_ref()], bump,
        constraint = !vesting.is_initialized @ VestingError::AlreadyInitialized,
        constraint = vesting.schedule.len() == schedules.len() @ VestingError::InvalidScheduleLen
    )]
    pub vesting: Account<'info, Vesting>,

    #[account(mut,
        constraint = vesting_token.owner == vesting.key() @ VestingError::InvalidVestingTokenAuthority,
        constraint = vesting_token.delegate.is_none() @ VestingError::InvalidVestingTokenDelegateAuthority,
        constraint = vesting_token.close_authority.is_none() @ VestingError::InvalidVestingTokenCloseAuthority
    )]
    pub vesting_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub source_token: Account<'info, TokenAccount>,

    pub source_authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(seeds: [u8; 31])]
pub struct Unlock<'info> {
    #[account(mut, seeds = [seeds.as_ref()], bump,
        constraint = vesting.is_initialized @ VestingError::NotInitialized,
    constraint = vesting.destination_address == destination_token.key() @ VestingError::InvalidDestination
    )]
    pub vesting: Account<'info, Vesting>,

    #[account(mut,
        constraint = vesting_token.owner == vesting.key() @ VestingError::InvalidVestingTokenAuthority
    )]
    pub vesting_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub destination_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(seeds: [u8; 31])]
pub struct ChangeDestination<'info> {
    #[account(mut, seeds = [seeds.as_ref()], bump,
        constraint = vesting.is_initialized @ VestingError::NotInitialized,
        constraint = vesting.destination_address == current_destination_token.key() @ VestingError::InvalidDestination
    )]
    pub vesting: Account<'info, Vesting>,

    #[account(constraint = current_destination_token.owner == destination_authority.key() @ VestingError::InvalidDestinationAuthority)]
    pub current_destination_token: Account<'info, TokenAccount>,
    pub destination_authority: Signer<'info>,
    pub new_destination_token: Account<'info, TokenAccount>,
}

#[account]
pub struct Vesting {
    pub destination_address: Pubkey,
    pub mint_address: Pubkey,
    pub is_initialized: bool,
    pub schedule: Vec<Schedule>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Schedule {
    // Schedule release time in unix timestamp
    pub release_time: u64,
    pub amount: u64,
}

#[error_code]
pub enum VestingError {
    #[msg("Cannot overwrite an existing vesting contract.")]
    AlreadyInitialized,
    #[msg("The vesting account isn't initialized.")]
    NotInitialized,
    #[msg("The vesting token account should be owned by the vesting account.")]
    InvalidVestingTokenAuthority,
    #[msg("The vesting token account should not have a delegate authority")]
    InvalidVestingTokenDelegateAuthority,
    #[msg("The vesting token account should not have a close authority")]
    InvalidVestingTokenCloseAuthority,
    #[msg("The source token account has insufficient funds.")]
    InsufficientFunds,
    #[msg("The schedule vector should have len defined during initialize.")]
    InvalidScheduleLen,
    #[msg("Total amount overflows u64")]
    TotalAmountOverflow,
    #[msg("Contract destination account does not matched provided account")]
    InvalidDestination,
    #[msg("Vesting contract has not yet reached release time")]
    ReleaseTimeNotYetReached,
    #[msg("The current destination token account isn't owned by the provided owner")]
    InvalidDestinationAuthority,
}

fn calc_vesting_account_size(number_of_schedules: u32) -> usize {
    8 // discriminator
    + std::mem::size_of::<Pubkey>() // destination_address
    + std::mem::size_of::<Pubkey>() // mint_address
    + 1 // is_initialized
    + 4 + (number_of_schedules as usize) * 2 * std::mem::size_of::<u64>() // schedule
}

fn total_amount(schedules: &Vec<Schedule>) -> Result<u64> {
    schedules
        .iter()
        .try_fold(0u64, |sum, s| sum.checked_add(s.amount))
        .ok_or_else(|| VestingError::TotalAmountOverflow.into())
}

fn total_amount_to_transfer(schedules: &Vec<Schedule>, timestamp: anchor_lang::solana_program::clock::UnixTimestamp) -> u64 {
    schedules
        .iter()
        .filter_map(|s| if timestamp as u64 >= s.release_time { Some(s.amount) } else { None })
        .sum()
}

fn reset_released_amount(schedules: &mut Vec<Schedule>, timestamp: anchor_lang::solana_program::clock::UnixTimestamp) {
    schedules
        .iter_mut()
        .filter_map(|s| if timestamp as u64 >= s.release_time {Some(s.amount.borrow_mut())} else {None} )
        .for_each(|amount|*amount = 0);
}