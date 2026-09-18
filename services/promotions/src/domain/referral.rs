//! Referrals: an account shares its code; a new account enters it; when the new
//! account's first move completes, the one who shared it is credited.
//!
//! Paying on a completed move rather than on sign-up is what stops a referral
//! code being farmed with throwaway accounts.

use uuid::Uuid;

/// No 0/O or 1/I/L: a code is read aloud and typed from a screenshot.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// An 8-character code from random bits.
pub fn referral_code(seed: Uuid) -> String {
    let mut n = seed.as_u128();
    let mut out = String::with_capacity(8);
    for _ in 0..8 {
        out.push(ALPHABET[(n % ALPHABET.len() as u128) as usize] as char);
        n /= ALPHABET.len() as u128;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimRefusal {
    UnknownCode,
    OwnCode,
    /// Referrals are for new accounts: one that has already booked is not.
    NotNew,
    AlreadyReferred,
}

impl ClaimRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownCode => "UNKNOWN_REFERRAL_CODE",
            Self::OwnCode => "OWN_REFERRAL_CODE",
            Self::NotNew => "NOT_A_NEW_ACCOUNT",
            Self::AlreadyReferred => "ALREADY_REFERRED",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::UnknownCode => "That isn't a referral code we recognise.",
            Self::OwnCode => "That's your own code — share it with a friend instead.",
            Self::NotNew => "Referral codes are for new accounts, before their first booking.",
            Self::AlreadyReferred => "Your account already has a referral.",
        }
    }
}

/// May `invitee` claim the code that belongs to `referrer`?
pub fn may_claim(
    referrer: Option<Uuid>,
    invitee: Uuid,
    invitee_moves_booked: i64,
    invitee_already_referred: bool,
) -> Result<Uuid, ClaimRefusal> {
    let referrer = referrer.ok_or(ClaimRefusal::UnknownCode)?;
    if referrer == invitee {
        return Err(ClaimRefusal::OwnCode);
    }
    if invitee_already_referred {
        return Err(ClaimRefusal::AlreadyReferred);
    }
    if invitee_moves_booked > 0 {
        return Err(ClaimRefusal::NotNew);
    }
    Ok(referrer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_eight_unambiguous_characters() {
        let c = referral_code(Uuid::from_u128(123_456_789_012_345));
        assert_eq!(c.len(), 8);
        assert!(c.chars().all(|ch| ALPHABET.contains(&(ch as u8))));
        assert_ne!(c, referral_code(Uuid::from_u128(123_456_789_012_346)));
    }

    #[test]
    fn a_new_account_claims_a_friends_code() {
        let (friend, me) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(may_claim(Some(friend), me, 0, false), Ok(friend));
    }

    #[test]
    fn the_rules_that_stop_farming() {
        let (friend, me) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(may_claim(None, me, 0, false), Err(ClaimRefusal::UnknownCode));
        assert_eq!(may_claim(Some(me), me, 0, false), Err(ClaimRefusal::OwnCode));
        assert_eq!(may_claim(Some(friend), me, 1, false), Err(ClaimRefusal::NotNew));
        assert_eq!(may_claim(Some(friend), me, 0, true), Err(ClaimRefusal::AlreadyReferred));
    }
}
