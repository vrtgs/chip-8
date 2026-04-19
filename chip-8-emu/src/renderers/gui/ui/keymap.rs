use std::collections::HashMap;
use iced::keyboard::key;
use iced::keyboard::key::Code;
use chip_8_core::InputIndex;


#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Action {
    GameInput(InputIndex),
    ReloadRom,
    TogglePause,
}

impl Action {
    pub const TOTAL_ACTIONS: usize = InputIndex::TOTAL_INDICES + 2;

    pub const fn as_usize(self) -> usize {
        match self {
            Action::GameInput(input) => input.as_usize(),
            Action::ReloadRom => InputIndex::TOTAL_INDICES,
            Action::TogglePause => InputIndex::TOTAL_INDICES + 1,
        }
    }
}


pub(super) struct Keymap {
    // # invariants
    //
    // * `key_map[k] = i` iff `input_to_key[i] = Some(k)`
    // * if `input_to_key[i] = None`; for all k `key_map[k] != i`
    key_to_input: HashMap<key::Physical, Action>,
    input_to_key: [Option<key::Physical>; Action::TOTAL_ACTIONS],
}

impl Default for Keymap {
    fn default() -> Self {
        macro_rules! make_map {
            {
                @final;
                $(($key: expr) <=> $action: expr),*
            } => {{
                let input_to_key = const {
                    let mut keys = [None; Action::TOTAL_ACTIONS];

                    $({
                        let index = $action;
                        let key_slot = &mut keys[index.as_usize()];
                        assert!(
                            key_slot.is_none(),
                            "mapping index to key collision; too many keys map to an index"
                        );
                        let key = $key;
                        assert!(
                            !matches!(key, key::Physical::Code(Code::Escape)),
                            "can't bind the escape key to any inputs"
                        );
                        *key_slot = Some(key)
                    })*

                    let mut i = 0;
                    while i < Action::TOTAL_ACTIONS {
                        assert!(keys[i].is_some(), "there are unmapped indices");
                        i += 1;
                    }

                    keys
                };

                let forward_mapping = [$(($key, $action)),*];
                let expected_len = forward_mapping.len();
                let key_map = HashMap::<key::Physical, Action>::from(forward_mapping);

                // TODO const time equality checks
                assert_eq!(
                    key_map.len(),
                    expected_len,
                    "a single key was mapped to multiple input indicies"
                );

                Keymap {
                    key_to_input:key_map,
                    input_to_key
                }
            }};

            {
                reload = $reload_key: ident;
                toggle_pause = $pause_key: ident;
                keypad: $($keypad_key: ident <=> $index: ident),+ $(,)?
            } => {
                make_map! {
                    @final;

                    (key::Physical::Code(Code::$reload_key)) <=> Action::ReloadRom,
                    (key::Physical::Code(Code::$pause_key)) <=> Action::TogglePause,

                    $(
                    (key::Physical::Code(Code::$keypad_key))
                        <=> Action::GameInput(InputIndex::$index)
                    ),+
                }
            };
        }

        make_map! {
            reload = Delete;
            toggle_pause = Space;

            keypad:
            Digit1 <=> _1,
            Digit2 <=> _2,
            Digit3 <=> _3,
            Digit4 <=> _C,

            KeyQ <=> _4,
            KeyW <=> _5,
            KeyE <=> _6,
            KeyR <=> _D,

            KeyA <=> _7,
            KeyS <=> _8,
            KeyD <=> _9,
            KeyF <=> _E,

            KeyZ <=> _A,
            KeyX <=> _0,
            KeyC <=> _B,
            KeyV <=> _F,
        }
    }
}

impl Keymap {
    pub fn remap(&mut self, key: key::Physical, input: Action) {
        // unbind the input
        if let Some(old_key) = self.input_to_key[input.as_usize()].take() {
            let old_mapping = self.key_to_input.remove(&old_key);
            assert_eq!(old_mapping, Some(input))
        }

        // mapping an input to escape means unbind
        if let key::Physical::Code(Code::Escape) = key {
            return;
        }

        // rebind to key
        let new_key_old_input = self.key_to_input.insert(key, input);
        if let Some(old_input) = new_key_old_input {
            let old_key = self.input_to_key[old_input.as_usize()].take();
            assert_eq!(old_key, Some(key));
        }

        self.input_to_key[input.as_usize()] = Some(key);
    }

    pub fn get(&self, key: key::Physical) -> Option<Action> {
        self.key_to_input.get(&key).copied()
    }

    pub fn key_for(&self, input: Action) -> Option<key::Physical> {
        self.input_to_key[input.as_usize()]
    }
}
