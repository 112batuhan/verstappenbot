use vosk::{CompleteResult, Model, Recognizer};

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum ModelLanguage {
    ENGLISH,
    TURKISH,
    DUTCH,
}

pub struct SpeechToText {
    recognizer: Recognizer,
    active: bool,
    words: Vec<String>,
    phrases: Vec<String>,
    language: ModelLanguage,
}

impl SpeechToText {
    pub fn new_with_grammar(
        model: &Model,
        language: ModelLanguage,
        words: &[String],
        phrases: &[String],
    ) -> Self {
        let grammar: Vec<String> = words.iter().chain(phrases.iter()).cloned().collect();
        let mut recognizer = Recognizer::new_with_grammar(model, 48000., &grammar)
            .expect("Could not create the Recognizer");
        recognizer.set_words(true);
        Self {
            recognizer,
            active: false,
            words: words.to_vec(),
            phrases: phrases.to_vec(),
            language,
        }
    }

    pub fn listen(&mut self, data: &[i16]) {
        let data = stereo_to_mono(data);
        self.recognizer.accept_waveform(&data);
        self.active = true;
    }

    // be cautious as there are a lot of "word" here.
    // One is Vosk result word, other one is words we are looking for.
    pub fn finalise(&mut self) -> Option<(String, ModelLanguage)> {
        if self.active {
            self.active = false;
            let result = self.recognizer.final_result();
            if let CompleteResult::Single(result) = result {
                let word_result = result.result.iter().find(|word| {
                    println!("{:?}", word);
                    word.conf > 0.999 && self.words.iter().any(|w| w == word.word)
                });
                if let Some(word) = word_result {
                    return Some((word.word.to_string(), self.language));
                }

                for phrase in &self.phrases {
                    if result.text.contains(phrase) {
                        return Some((phrase.to_string(), self.language));
                    }
                }
            }
        }
        None
    }
}

pub fn stereo_to_mono(input_data: &[i16]) -> Vec<i16> {
    let mut result = Vec::with_capacity(input_data.len() / 2);
    result.extend(
        input_data
            .chunks_exact(2)
            .map(|chunk| chunk[0] / 2 + chunk[1] / 2),
    );

    result
}
