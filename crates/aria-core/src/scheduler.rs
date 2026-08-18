use crate::action::Action;

/// Scheduler — policy layer, NOT Spec.
///
/// Outputs which action to take next. Only applies Spec transitions;
/// Engine enforces invariants.
#[derive(Debug, Clone)]
pub struct Scheduler {
    /// Current position in the schedule
    pos: usize,
    /// Pre-computed action sequence
    schedule: Vec<Action>,
    /// Consecutive stutter counter
    stutter_count: u64,
    /// Maximum allowed consecutive stutters (𝐂5)
    stutter_k: u64,
}

impl Scheduler {
    /// Build a scheduler from a schedule string.
    ///
    /// "opmd" → the preferred Φ-cycle: O→P→M→D (𝐂4).
    /// Custom string: each char maps to an action (o=OpticalStep, p=Predict, m=Match, d=Diffuse, s=Stutter).
    pub fn from_string(schedule: &str, stutter_k: u64) -> Result<Self, String> {
        let actions = Self::parse_schedule(schedule)?;
        Ok(Scheduler {
            pos: 0,
            schedule: actions,
            stutter_count: 0,
            stutter_k,
        })
    }

    fn parse_schedule(s: &str) -> Result<Vec<Action>, String> {
        let lower = s.to_lowercase();
        let mut actions = Vec::new();

        if lower == "opmd" {
            return Ok(Action::PHI_CYCLE.to_vec());
        }

        for ch in lower.chars() {
            match ch {
                'o' => actions.push(Action::OpticalStep),
                'p' => actions.push(Action::Predict),
                'm' => actions.push(Action::Match),
                'd' => actions.push(Action::Diffuse),
                's' => actions.push(Action::Stutter),
                c => return Err(format!("unknown action character: '{c}'")),
            }
        }

        if actions.is_empty() {
            return Err("empty schedule".into());
        }

        Ok(actions)
    }

    /// Get the next action in the repeating schedule.
    pub fn next_action(&mut self) -> Action {
        let action = self.schedule[self.pos];
        self.pos = (self.pos + 1) % self.schedule.len();

        // Track stutter budget (𝐂5)
        if action == Action::Stutter {
            self.stutter_count += 1;
        } else {
            self.stutter_count = 0;
        }

        action
    }

    /// Get the next action, but skip Stutter if budget exceeded (𝐂5).
    pub fn next_action_budgeted(&mut self) -> Action {
        if self.stutter_count >= self.stutter_k {
            // Force a productive action
            self.stutter_count = 0;
            // Skip to next non-stutter in schedule
            let mut steps = 0;
            loop {
                let action = self.schedule[self.pos];
                self.pos = (self.pos + 1) % self.schedule.len();
                steps += 1;
                if action != Action::Stutter {
                    return action;
                }
                if steps >= self.schedule.len() {
                    // All stutters — return OpticalStep as fallback (𝐂7)
                    return Action::OpticalStep;
                }
            }
        }
        self.next_action()
    }

    /// Peek at the next action without advancing.
    pub fn peek(&self) -> Action {
        self.schedule[self.pos]
    }

    /// Number of consecutive stutters so far.
    pub fn stutter_count(&self) -> u64 {
        self.stutter_count
    }

    /// Reset position.
    pub fn reset(&mut self) {
        self.pos = 0;
        self.stutter_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opmd_schedule() {
        let mut s = Scheduler::from_string("opmd", 2).unwrap();
        assert_eq!(s.next_action(), Action::OpticalStep);
        assert_eq!(s.next_action(), Action::Predict);
        assert_eq!(s.next_action(), Action::Match);
        assert_eq!(s.next_action(), Action::Diffuse);
        // Repeats
        assert_eq!(s.next_action(), Action::OpticalStep);
    }

    #[test]
    fn custom_schedule() {
        let mut s = Scheduler::from_string("opd", 2).unwrap();
        assert_eq!(s.next_action(), Action::OpticalStep);
        assert_eq!(s.next_action(), Action::Predict);
        assert_eq!(s.next_action(), Action::Diffuse);
    }
}
