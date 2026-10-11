//! What the staff say, in each language: the page's (index.html says which) until the player picks
//! another. `{name}`, `{game}`, `{count}` and `{players}` stand for what fills them in, and game
//! titles and people's names go in as they are, untranslated. The font has Latin-1 only (main.rs),
//! so no curly quotes, dashes or ellipses.

/// Everything the staff say, in one language.
pub struct Lines {
    /// The language's code (a page's language starts with it) and its own name for itself.
    pub code: &'static str,
    pub language: &'static str,
    /// The hint next to someone on the staff, and the touch screen's button for it.
    pub talk_to: &'static str,
    pub talk: &'static str,
    pub bye: &'static str,
    pub back: &'static str,
    pub thanks: &'static str,
    /// How each bartender and cleaner says hello, in the order staff.rs lists them.
    pub bartender_hello: [&'static str; 3],
    pub cleaner_hello: [&'static str; 3],
    pub ask_play: &'static str,
    pub ask_busy: &'static str,
    pub ask_friends: &'static str,
    pub ask_controls: &'static str,
    /// The way to the other languages: in this one, and in English too for whoever can't read it.
    pub ask_language: &'static str,
    pub genres_line: &'static str,
    /// Fighting, action, racing, sports, classic, other (a game without a genre), the tables.
    pub genres: [&'static str; 7],
    pub genre_line: &'static str,
    /// Pool, air hockey, shuffleboard, darts, pinball.
    pub tables: [&'static str; 5],
    pub there: &'static str,
    pub busy_none: &'static str,
    pub busy_line: &'static str,
    pub busy_entry: &'static str,
    pub friends: &'static str,
    pub ask_invite: &'static str,
    pub invite: &'static str,
    pub controls: &'static str,
    pub controls_touch: &'static str,
    pub ask_all_controls: &'static str,
    pub language_line: &'static str,
    pub ask_tip: &'static str,
    pub another_tip: &'static str,
    pub tips: [&'static str; 8],
    pub ask_bartender: &'static str,
    pub bartender_is: &'static str,
}

impl Lines {
    /// Every line, in the order they're declared, for checking them all alike.
    #[cfg(test)]
    pub fn all(&self) -> Vec<&'static str> {
        let mut all = vec![
            self.language,
            self.talk_to,
            self.talk,
            self.bye,
            self.back,
            self.thanks,
        ];
        all.extend(self.bartender_hello);
        all.extend(self.cleaner_hello);
        all.extend([
            self.ask_play,
            self.ask_busy,
            self.ask_friends,
            self.ask_controls,
            self.ask_language,
            self.genres_line,
        ]);
        all.extend(self.genres);
        all.push(self.genre_line);
        all.extend(self.tables);
        all.extend([
            self.there,
            self.busy_none,
            self.busy_line,
            self.busy_entry,
            self.friends,
            self.ask_invite,
            self.invite,
            self.controls,
            self.controls_touch,
            self.ask_all_controls,
            self.language_line,
            self.ask_tip,
            self.another_tip,
        ]);
        all.extend(self.tips);
        all.extend([self.ask_bartender, self.bartender_is]);
        all
    }
}

/// The languages, English first: what a page in any other language gets.
pub const LANGUAGES: [&Lines; 5] = [&ENGLISH, &SPANISH, &PORTUGUESE, &FRENCH, &GERMAN];

/// The lines for a page's language ("es-AR", "pt", "de-CH"), English if there are none.
pub fn for_language(code: &str) -> &'static Lines {
    let code = code.to_ascii_lowercase();
    LANGUAGES
        .into_iter()
        .find(|lines| code.split(['-', '_']).next() == Some(lines.code))
        .unwrap_or(&ENGLISH)
}

const ENGLISH: Lines = Lines {
    code: "en",
    language: "English",
    talk_to: "Talk to {name}",
    talk: "Talk",
    bye: "Bye",
    back: "Back",
    thanks: "Thanks!",
    bartender_hello: [
        "Hi, welcome in! I'm Rosa. What can I do for you?",
        "Evening. Name's Lou. What'll it be?",
        "Hey! Kenji here. First time at the bar?",
    ],
    cleaner_hello: [
        "Mind the wet floor, dear. Need something?",
        "Careful, I just mopped there. What's up?",
        "Oh, hello! Don't slip. Can I help?",
    ],
    ask_play: "What can I play?",
    ask_busy: "Who's playing right now?",
    ask_friends: "How do I play with friends?",
    ask_controls: "How do the controls work?",
    ask_language: "Language",
    genres_line: "{count} cabinets, plus the tables. What are you in the mood for?",
    genres: [
        "Fighting games",
        "Beat 'em ups and shooters",
        "Racing",
        "Sports",
        "Classics",
        "Something else",
        "The tables",
    ],
    genre_line: "Here's what we've got:",
    tables: ["Pool", "Air hockey", "Shuffleboard", "Darts", "Pinball"],
    there: "{game}? Right this way: follow the arrow.",
    busy_none: "Nobody's playing just now. Pick any game: whoever sits next to you plays with you.",
    busy_line: "Right now:",
    busy_entry: "{game}: {count} playing",
    friends: "Sit at the same cabinet or table as someone and you play together, online. When \
              every seat is taken, E watches instead, and F watches any time. Sitting together, \
              you can talk: M turns your microphone on and off.",
    ask_invite: "How do I bring friends?",
    invite: "Send them this page's address. Add ?room= and a name to it for a bar of your own: \
             only those you send it to come in.",
    controls: "Arrows or W A S D walk, Shift runs. E plays whatever you're next to. At a \
               cabinet, 5 puts in a coin, 1 starts and Esc stands you up. Y opens the chat, \
               where /name sets your name.",
    controls_touch: "Drag a thumb to walk. Play and Watch show up next to a game. At a cabinet, \
                     Coin, Start and Leave are on the screen, with the game's buttons.",
    ask_all_controls: "Show me all the controls",
    language_line: "Which language?",
    ask_tip: "Any tips?",
    another_tip: "Another one",
    tips: [
        "Hold Shift to run. Saves the legs.",
        "Next to a game someone's playing, F lets you watch.",
        "Type /name and your name in the chat, so people know who you are.",
        "Sitting with someone, press M to talk to them.",
        "Daytona USA races up to 8 at once, each at their own cabinet.",
        "Lightgun games, like Time Crisis II, aim with the mouse.",
        "At the pool table, hold the button longer for a harder shot.",
        "Driving games: /wheel in the chat tunes the steering.",
    ],
    ask_bartender: "Who can show me around?",
    bartender_is: "{name}, behind the counter, knows this place inside out. Follow the arrow.",
};

const SPANISH: Lines = Lines {
    code: "es",
    language: "Español",
    talk_to: "Hablar con {name}",
    talk: "Hablar",
    bye: "Adiós",
    back: "Volver",
    thanks: "¡Gracias!",
    bartender_hello: [
        "¡Hola, qué bueno verte! Soy Rosa. ¿En qué te ayudo?",
        "Buenas. Me llamo Lou. ¿Qué va a ser?",
        "¡Hola! Soy Kenji. ¿Primera vez en el bar?",
    ],
    cleaner_hello: [
        "Cuidado con el piso mojado, cielo. ¿Necesitas algo?",
        "Ojo, que acabo de limpiar ahí. ¿Qué tal?",
        "¡Ay, hola! No te resbales. ¿Te ayudo en algo?",
    ],
    ask_play: "¿Qué puedo jugar?",
    ask_busy: "¿Quién está jugando ahora?",
    ask_friends: "¿Cómo juego con amigos?",
    ask_controls: "¿Cómo son los controles?",
    ask_language: "Idioma (Language)",
    genres_line: "{count} máquinas, y además las mesas. ¿Qué tienes ganas de jugar?",
    genres: [
        "Juegos de pelea",
        "Beat 'em ups y disparos",
        "Carreras",
        "Deportes",
        "Clásicos",
        "Otros",
        "Las mesas",
    ],
    genre_line: "Esto es lo que tenemos:",
    tables: [
        "Billar",
        "Hockey de aire",
        "Shuffleboard",
        "Dardos",
        "Pinball",
    ],
    there: "¿{game}? Por aquí: sigue la flecha.",
    busy_none: "Ahora no está jugando nadie. Elige cualquier juego: quien se siente a tu lado \
                juega contigo.",
    busy_line: "Ahora mismo:",
    busy_entry: "{game}: {count} jugando",
    friends: "Siéntate en la misma máquina o mesa que alguien y juegan juntos, en línea. Si \
              todos los asientos están ocupados, E te deja mirar, y F mira cuando quieras. \
              Sentados juntos pueden hablar: M enciende y apaga tu micrófono.",
    ask_invite: "¿Cómo traigo a mis amigos?",
    invite: "Mándales la dirección de esta página. Agrégale ?room= y un nombre para tener un \
             bar propio: solo entra a quien se la mandes.",
    controls: "Las flechas o W A S D caminan, Shift corre. E juega a lo que tengas al lado. En \
               una máquina, 5 pone una moneda, 1 empieza y Esc te levanta. Y abre el chat, \
               donde /name cambia tu nombre.",
    controls_touch: "Arrastra un dedo para caminar. Junto a un juego aparecen Play y Watch. En \
                     una máquina tienes Coin, Start y Leave en la pantalla, con los botones del \
                     juego.",
    ask_all_controls: "Muéstrame todos los controles",
    language_line: "¿Qué idioma?",
    ask_tip: "¿Algún consejo?",
    another_tip: "Otro",
    tips: [
        "Mantén Shift para correr. Las piernas te lo agradecen.",
        "Junto a un juego en el que alguien está jugando, F te deja mirar.",
        "Escribe /name y tu nombre en el chat, así saben quién eres.",
        "Sentado con alguien, presiona M para hablar con esa persona.",
        "En Daytona USA corren hasta 8 a la vez, cada uno en su máquina.",
        "Los juegos de pistola, como Time Crisis II, se apuntan con el mouse.",
        "En el billar, mantén el botón más tiempo para un tiro más fuerte.",
        "En los juegos de carreras, /wheel en el chat ajusta el volante.",
    ],
    ask_bartender: "¿Quién me puede mostrar el lugar?",
    bartender_is: "{name}, detrás de la barra, conoce este lugar como nadie. Sigue la flecha.",
};

const PORTUGUESE: Lines = Lines {
    code: "pt",
    language: "Português",
    talk_to: "Falar com {name}",
    talk: "Falar",
    bye: "Tchau",
    back: "Voltar",
    thanks: "Valeu!",
    bartender_hello: [
        "Oi, que bom te ver! Sou a Rosa. Em que posso ajudar?",
        "Boa noite. Me chamo Lou. O que vai ser?",
        "E aí! Kenji aqui. Primeira vez no bar?",
    ],
    cleaner_hello: [
        "Cuidado com o chão molhado, viu? Precisa de algo?",
        "Opa, acabei de passar pano aí. Fala!",
        "Ah, oi! Não vá escorregar. Posso ajudar?",
    ],
    ask_play: "O que eu posso jogar?",
    ask_busy: "Quem está jogando agora?",
    ask_friends: "Como jogo com amigos?",
    ask_controls: "Como funcionam os controles?",
    ask_language: "Idioma (Language)",
    genres_line: "{count} máquinas, e ainda as mesas. Está a fim de jogar o quê?",
    genres: [
        "Jogos de luta",
        "Beat 'em ups e tiro",
        "Corrida",
        "Esportes",
        "Clássicos",
        "Outros",
        "As mesas",
    ],
    genre_line: "Olha só o que temos:",
    tables: ["Sinuca", "Air hockey", "Shuffleboard", "Dardos", "Pinball"],
    there: "{game}? Por aqui: siga a seta.",
    busy_none: "Ninguém jogando agora. Escolha qualquer jogo: quem sentar do seu lado joga com \
                você.",
    busy_line: "Agora mesmo:",
    busy_entry: "{game}: {count} jogando",
    friends: "Sente na mesma máquina ou mesa que alguém e vocês jogam juntos, online. Com todos \
              os lugares ocupados, E assiste, e F assiste quando quiser. Sentados juntos, vocês \
              podem conversar: M liga e desliga seu microfone.",
    ask_invite: "Como trago meus amigos?",
    invite: "Mande o endereço desta página. Junte ?room= e um nome para ter um bar só seu: só \
             entra quem receber.",
    controls: "Setas ou W A S D andam, Shift corre. E joga o que estiver do seu lado. Numa \
               máquina, 5 põe uma ficha, 1 começa e Esc levanta você. Y abre o chat, onde /name \
               muda seu nome.",
    controls_touch: "Arraste o dedo para andar. Perto de um jogo aparecem Play e Watch. Numa \
                     máquina, Coin, Start e Leave ficam na tela, com os botões do jogo.",
    ask_all_controls: "Mostre todos os controles",
    language_line: "Qual idioma?",
    ask_tip: "Alguma dica?",
    another_tip: "Mais uma",
    tips: [
        "Segure Shift para correr. Poupa as pernas.",
        "Perto de um jogo em que alguém está jogando, F deixa você assistir.",
        "Digite /name e seu nome no chat, para saberem quem você é.",
        "Sentado com alguém, aperte M para conversar.",
        "No Daytona USA correm até 8 de uma vez, cada um na sua máquina.",
        "Jogos de pistola, como Time Crisis II, miram com o mouse.",
        "Na sinuca, segure o botão mais tempo para uma tacada mais forte.",
        "Nos jogos de corrida, /wheel no chat ajusta o volante.",
    ],
    ask_bartender: "Quem pode me mostrar o lugar?",
    bartender_is: "{name}, atrás do balcão, conhece este lugar como ninguém. Siga a seta.",
};

const FRENCH: Lines = Lines {
    code: "fr",
    language: "Français",
    talk_to: "Parler à {name}",
    talk: "Parler",
    bye: "Au revoir",
    back: "Retour",
    thanks: "Merci !",
    bartender_hello: [
        "Salut, bienvenue ! Moi, c'est Rosa. Qu'est-ce que je te sers ?",
        "Bonsoir. Je m'appelle Lou. Qu'est-ce que ce sera ?",
        "Hé ! Moi, c'est Kenji. C'est ta première fois au bar ?",
    ],
    cleaner_hello: [
        "Attention, sol mouillé ! Tu as besoin de quelque chose ?",
        "Doucement, je viens de passer la serpillière. Quoi de neuf ?",
        "Oh, bonjour ! Ne glisse pas. Je peux t'aider ?",
    ],
    ask_play: "À quoi je peux jouer ?",
    ask_busy: "Qui joue en ce moment ?",
    ask_friends: "Comment jouer avec des amis ?",
    ask_controls: "Comment marchent les commandes ?",
    ask_language: "Langue (Language)",
    genres_line: "{count} bornes, et les tables en plus. Tu as envie de quoi ?",
    genres: [
        "Jeux de combat",
        "Beat 'em up et tir",
        "Course",
        "Sport",
        "Classiques",
        "Autres",
        "Les tables",
    ],
    genre_line: "Voilà ce qu'on a :",
    tables: [
        "Billard",
        "Air hockey",
        "Shuffleboard",
        "Fléchettes",
        "Flipper",
    ],
    there: "{game} ? Par ici : suis la flèche.",
    busy_none: "Personne ne joue pour l'instant. Choisis n'importe quel jeu : qui s'assoit à \
                côté de toi joue avec toi.",
    busy_line: "En ce moment :",
    busy_entry: "{game} : {count} en jeu",
    friends: "Assieds-toi à la même borne ou table que quelqu'un et vous jouez ensemble, en \
              ligne. Quand toutes les places sont prises, E regarde, et F regarde quand tu \
              veux. Assis ensemble, vous pouvez parler : M allume et coupe ton micro.",
    ask_invite: "Comment faire venir mes amis ?",
    invite: "Envoie-leur l'adresse de cette page. Ajoute ?room= et un nom pour un bar rien qu'à \
             vous : n'entrent que ceux à qui tu l'envoies.",
    controls: "Les flèches (ou Z Q S D) pour marcher, Shift pour courir. E joue à ce qui est à \
               côté de toi. À une borne, 5 met une pièce, 1 lance la partie et Esc te lève. Y \
               ouvre le chat, où /name change ton nom.",
    controls_touch: "Fais glisser un doigt pour marcher. Play et Watch apparaissent près d'un \
                     jeu. À une borne, Coin, Start et Leave sont à l'écran, avec les boutons du \
                     jeu.",
    ask_all_controls: "Montre-moi toutes les commandes",
    language_line: "Quelle langue ?",
    ask_tip: "Un conseil ?",
    another_tip: "Un autre",
    tips: [
        "Garde Shift enfoncé pour courir. Ça épargne les jambes.",
        "À côté d'un jeu où quelqu'un joue, F te permet de regarder.",
        "Tape /name et ton nom dans le chat, pour qu'on sache qui tu es.",
        "Assis avec quelqu'un, appuie sur M pour lui parler.",
        "À Daytona USA, on court jusqu'à 8 à la fois, chacun à sa borne.",
        "Les jeux de pistolet, comme Time Crisis II, se visent à la souris.",
        "Au billard, garde le bouton enfoncé plus longtemps pour tirer plus fort.",
        "Jeux de course : /wheel dans le chat règle le volant.",
    ],
    ask_bartender: "Qui peut me faire visiter ?",
    bartender_is: "{name}, derrière le comptoir, connaît l'endroit comme sa poche. Suis la \
                   flèche.",
};

const GERMAN: Lines = Lines {
    code: "de",
    language: "Deutsch",
    talk_to: "Mit {name} reden",
    talk: "Reden",
    bye: "Tschüss",
    back: "Zurück",
    thanks: "Danke!",
    bartender_hello: [
        "Hallo, willkommen! Ich bin Rosa. Was darf's sein?",
        "'n Abend. Ich bin Lou. Was soll's sein?",
        "Hey! Ich bin Kenji. Zum ersten Mal hier?",
    ],
    cleaner_hello: [
        "Vorsicht, der Boden ist nass! Brauchst du was?",
        "Achtung, da hab ich gerade gewischt. Was gibt's?",
        "Oh, hallo! Nicht ausrutschen. Kann ich helfen?",
    ],
    ask_play: "Was kann ich spielen?",
    ask_busy: "Wer spielt gerade?",
    ask_friends: "Wie spiele ich mit Freunden?",
    ask_controls: "Wie funktioniert die Steuerung?",
    ask_language: "Sprache (Language)",
    genres_line: "{count} Automaten, dazu die Tische. Worauf hast du Lust?",
    genres: [
        "Prügelspiele",
        "Beat 'em ups und Shooter",
        "Rennspiele",
        "Sport",
        "Klassiker",
        "Anderes",
        "Die Tische",
    ],
    genre_line: "Das haben wir:",
    tables: ["Billard", "Airhockey", "Shuffleboard", "Darts", "Flipper"],
    there: "{game}? Hier entlang: folg dem Pfeil.",
    busy_none: "Gerade spielt niemand. Such dir ein Spiel aus: Wer sich neben dich setzt, spielt \
                mit dir.",
    busy_line: "Gerade:",
    busy_entry: "{game}: {count} spielen",
    friends: "Setz dich an denselben Automaten oder Tisch wie jemand anderes, dann spielt ihr \
              zusammen, online. Sind alle Plätze besetzt, schaut E zu, und F schaut jederzeit \
              zu. Wer zusammen sitzt, kann reden: M schaltet dein Mikrofon an und aus.",
    ask_invite: "Wie hole ich Freunde dazu?",
    invite: "Schick ihnen die Adresse dieser Seite. Häng ?room= und einen Namen an, dann habt \
             ihr eine eigene Bar: Rein kommt nur, wem du sie schickst.",
    controls: "Pfeiltasten oder W A S D zum Gehen, Shift zum Rennen. E spielt, was neben dir \
               steht. Am Automaten wirft 5 eine Münze ein, 1 startet und Esc steht auf. Y öffnet \
               den Chat, wo /name deinen Namen ändert.",
    controls_touch: "Zieh einen Finger zum Gehen. Neben einem Spiel erscheinen Play und Watch. \
                     Am Automaten sind Coin, Start und Leave auf dem Bildschirm, mit den Knöpfen \
                     des Spiels.",
    ask_all_controls: "Zeig mir die ganze Steuerung",
    language_line: "Welche Sprache?",
    ask_tip: "Hast du einen Tipp?",
    another_tip: "Noch einen",
    tips: [
        "Halt Shift gedrückt, um zu rennen. Schont die Beine.",
        "Neben einem Spiel, das jemand spielt, kannst du mit F zuschauen.",
        "Schreib /name und deinen Namen in den Chat, damit man weiß, wer du bist.",
        "Sitzt du mit jemandem zusammen, drück M, um zu reden.",
        "Bei Daytona USA fahren bis zu 8 gleichzeitig, jeder an seinem Automaten.",
        "Lightgun-Spiele wie Time Crisis II zielt man mit der Maus.",
        "Am Billardtisch: Je länger du die Taste hältst, desto härter der Stoß.",
        "Rennspiele: /wheel im Chat stellt die Lenkung ein.",
    ],
    ask_bartender: "Wer kann mich herumführen?",
    bartender_is: "{name} hinter der Theke kennt den Laden in- und auswendig. Folg dem Pfeil.",
};

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(line: &str) -> Vec<&str> {
        let mut found: Vec<&str> = line
            .match_indices('{')
            .filter_map(|(at, _)| line[at..].find('}').map(|end| &line[at..=at + end]))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn every_language_says_everything_with_the_same_blanks_in_the_bars_font() {
        let english = ENGLISH.all();
        for lines in LANGUAGES {
            let all = lines.all();
            assert_eq!(all.len(), english.len());
            for (line, original) in all.iter().zip(&english) {
                assert_eq!(placeholders(line), placeholders(original), "{line}");
                assert!(
                    line.chars().all(|c| (c as u32) < 0x100),
                    "not Latin-1: {line}"
                );
                assert!(!line.contains("  "), "a double space: {line}");
            }
        }
    }

    #[test]
    fn a_page_gets_its_own_language_or_english() {
        assert_eq!(for_language("es-AR").code, "es");
        assert_eq!(for_language("pt_BR").code, "pt");
        assert_eq!(for_language("DE").code, "de");
        assert_eq!(for_language("ja-JP").code, "en");
        assert_eq!(for_language("").code, "en");
    }
}
