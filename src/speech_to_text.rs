use vosk::{CompleteResult, Model, Recognizer};

pub fn stereo_to_mono(input_data: &[i16]) -> Vec<i16> {
    let mut result = Vec::with_capacity(input_data.len() / 2);
    result.extend(
        input_data
            .chunks_exact(2)
            .map(|chunk| chunk[0] / 2 + chunk[1] / 2),
    );

    result
}

pub struct SpeechToText {
    recognizer: Recognizer,
    active: bool,
}

impl SpeechToText {
    pub fn new(model: &Model) -> Self {
        let recognizer = Recognizer::new(&model, 48000.).expect("Could not create the Recognizer");

        Self {
            recognizer,
            active: false,
        }
    }
    pub fn new_with_grammar(model: &Model, grammar: &[&str]) -> Self {
        let mut recognizer = Recognizer::new_with_grammar(&model, 48000., grammar)
            .expect("Could not create the Recognizer");
        recognizer.set_words(true);
        Self {
            recognizer,
            active: false,
        }
    }

    pub fn listen(&mut self, data: &[i16]) {
        let data = stereo_to_mono(data);
        self.recognizer.accept_waveform(&data);
        self.active = true;
    }

    // TODO: Ugly
    pub fn finalise(&mut self) -> bool {
        if self.active {
            let result = self.recognizer.final_result();
            if let CompleteResult::Single(result) = result {
                return result.result.iter().any(|word| {
                    println!("{:?}", word);
                    word.conf > 0.95 && word.word == "intihar"
                });
            }
            self.active = false;
        }
        false
    }
}
