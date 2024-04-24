Handling of join, leave and moving between channels are a bit decentralised. 

Note: By handler, we mean the `Receiver` struct in `event.rs`. This struct implements songbird's event handler.

- Join is handled in commands. 
- If join command is used when bot already connected to a channel, old handler is removed and new handler is added.
- If bot is dragged into a new channel (aka channel movement), then joining a new channel doesn't reset the handler. Instead the recognizers and user id's that are in the handler gets reset in `ClientDisconnect` event.
- Leave command only disconnects the bot. It doesn't drop the handler.
- Handler is dropped when `VoiceStateUpdate` event picks up the bot and sees that the bot has left the channel. This also covers the manual disconnects.

Sounds robust. Until something will eventually break as always.
