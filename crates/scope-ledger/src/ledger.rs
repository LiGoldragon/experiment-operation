//! Identical operation bodies for both store owners.
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Account {
    pub name: String,
    pub balance: i64,
}
#[derive(Debug)]
pub struct Deposit {
    pub name: String,
    pub amount: i64,
}
#[derive(Debug)]
pub struct Withdraw {
    pub name: String,
    pub amount: i64,
}
#[derive(Debug)]
pub struct Transfer {
    pub from: String,
    pub to: String,
    pub amount: i64,
}
impl Memorizable for Account {
    const TABLE: &'static str = "Account";
    fn identity(&self) -> String {
        self.name.clone()
    }
    fn encode(&self) -> Result<Vec<u8>, Failure> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map(|b| b.to_vec())
            .map_err(|e| Failure::Storage(e.to_string()))
    }
    fn decode(bytes: &[u8]) -> Result<Self, Failure> {
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)
            .map_err(|e| Failure::Storage(e.to_string()))
    }
}
impl Operable for Deposit {
    type Outcome = Result<Account, Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        if self.amount < 0 {
            return Err(Failure::InvalidAmount);
        }
        let mut account = scope
            .run(Recall::<Account> {
                key: self.name.clone(),
                record: PhantomData,
            })
            .await?
            .ok_or(Failure::Missing(self.name))?;
        account.balance = account
            .balance
            .checked_add(self.amount)
            .ok_or(Failure::Overflow)?;
        scope.run(Remember(account.clone())).await?;
        Ok(account)
    }
}
impl Operable for Withdraw {
    type Outcome = Result<Account, Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        if self.amount < 0 {
            return Err(Failure::InvalidAmount);
        }
        let mut account = scope
            .run(Recall::<Account> {
                key: self.name.clone(),
                record: PhantomData,
            })
            .await?
            .ok_or(Failure::Missing(self.name))?;
        if account.balance < self.amount {
            return Err(Failure::Insufficient(account.balance));
        }
        account.balance -= self.amount;
        scope.run(Remember(account.clone())).await?;
        Ok(account)
    }
}
impl Operable for Transfer {
    type Outcome = Result<(), Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        // Deliberately follows the sketch's two separate changes, exposing partial completion.
        scope
            .run(Withdraw {
                name: self.from,
                amount: self.amount,
            })
            .await?;
        scope
            .run(Deposit {
                name: self.to,
                amount: self.amount,
            })
            .await?;
        Ok(())
    }
}
impl Textualizable for Account {
    fn datom(&self) -> String {
        format!("{{ «{}» {} }}", self.name, self.balance)
    }
}
impl Textualizable for Deposit {
    fn datom(&self) -> String {
        format!("Deposit.{{ «{}» {} }}", self.name, self.amount)
    }
}
impl Textualizable for Withdraw {
    fn datom(&self) -> String {
        format!("Withdraw.{{ «{}» {} }}", self.name, self.amount)
    }
}
impl Textualizable for Transfer {
    fn datom(&self) -> String {
        format!(
            "Transfer.{{ «{}» «{}» {} }}",
            self.from, self.to, self.amount
        )
    }
}
