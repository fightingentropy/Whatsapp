//! Chat policy belongs to the archive; wire encoding stays in whatsapp-rust.

use super::*;
use crate::archive::DisappearingTimer;

pub(super) struct SendState {
    pub generation: u64,
    pub timer: Option<DisappearingTimer>,
    pub retry: bool,
}

pub(super) fn timer(seconds: u32, updated_at: i64) -> Option<DisappearingTimer> {
    (seconds <= i32::MAX as u32 && updated_at >= 0).then_some(DisappearingTimer {
        seconds,
        updated_at,
    })
}

pub(super) fn group_duration(
    setting: Option<whatsapp_rust::GroupEphemeralSettings>,
) -> Result<u32, String> {
    // An absent node in full metadata means off. An incomplete node does not.
    match setting {
        None => Ok(0),
        Some(setting) => setting
            .expiration
            .filter(|seconds| *seconds <= i32::MAX as u32)
            .ok_or_else(|| {
                "The group returned an incomplete disappearing-message timer. Retry.".to_owned()
            }),
    }
}

pub(super) fn from_message(base: &wa::Message, at: i64) -> Option<DisappearingTimer> {
    if let Some(protocol) = base.protocol_message.as_option() {
        use wa::message::protocol_message::Type;
        return match protocol.r#type {
            Some(Type::EPHEMERAL_SETTING | Type::EPHEMERAL_SYNC_RESPONSE) => timer(
                protocol.ephemeral_expiration?,
                protocol.ephemeral_setting_timestamp.unwrap_or(at),
            ),
            _ => None,
        };
    }
    let context = context_of(base)?;
    if forwarded_of(base) {
        return None;
    }
    // Missing expiration is not an off switch: older clients can omit it even
    // in a timed chat. Only an explicit setting/zero can disable a known timer.
    timer(
        context.expiration?,
        context.ephemeral_setting_timestamp.unwrap_or(at),
    )
}

pub(super) fn clear_timer(message: &mut wa::Message) {
    // Preserve captions, mentions, quote and forwarding fields.
    if let Some(mut context) = context_of(message).cloned() {
        context.expiration = None;
        context.ephemeral_setting_timestamp = None;
        context.ephemeral_shared_secret = None;
        context.disappearing_mode = Default::default();
        message.set_context_info(context);
    }
}

impl Worker {
    pub(super) fn remember_timer(&self, chat: &str, timer: DisappearingTimer) {
        if let Err(error) = self.archive.set_chat_timer(chat, timer) {
            log::warn!("could not save a disappearing-message timer: {error}");
        }
    }

    fn outgoing_timer(&self, chat: &str) -> crate::archive::Result<Option<DisappearingTimer>> {
        let known = self.archive.chat_timer(chat)?;
        if known.is_some() || ChatKind::from_id(chat) != ChatKind::Direct {
            return Ok(known);
        }
        if let Some((timestamp, Some(raw))) = self.archive.latest_timer_source(chat)?
            && let Ok(message) = wa::Message::decode_from_slice(&raw)
        {
            let base = message.get_base_message();
            if let Some(update) = from_message(base, timestamp) {
                self.archive.set_chat_timer(chat, update)?;
                return Ok(Some(update));
            }
        }
        // Never reach past a newer untimed/unknown message to resurrect a timer
        // that may have been disabled before this upgrade.
        Ok(None)
    }

    pub(super) fn queue_send(
        &self,
        client: Arc<Client>,
        chat: ChatId,
        jid: Jid,
        id: String,
        message: wa::Message,
        retry: bool,
    ) {
        let timer = match self.outgoing_timer(&chat) {
            Ok(timer) => timer,
            Err(_) => {
                let _ = self.commands.send(Command::Sent {
                    chat,
                    id,
                    error: Some(
                        "Could not read the chat's disappearing-message timer. Retry.".into(),
                    ),
                });
                return;
            }
        };
        tokio::spawn(send_outgoing(
            client,
            self.commands.clone(),
            chat,
            jid,
            id,
            message,
            SendState {
                generation: self.bot_generation,
                timer,
                retry,
            },
        ));
    }

    pub(super) fn prepare_send(
        &self,
        chat: &str,
        id: &str,
        raw: &[u8],
        previous_timer: Option<DisappearingTimer>,
        group_timer: Option<DisappearingTimer>,
        retry: bool,
    ) -> Result<Vec<u8>, String> {
        let storage_error = || "Could not save the message's disappearing timer. Retry.".to_owned();
        let mut current = self.archive.chat_timer(chat).map_err(|_| storage_error())?;
        if let Some(fetched) = group_timer
            && (current == previous_timer
                || current.is_none_or(|known| known.updated_at < fetched.updated_at))
        {
            // An old history snapshot arriving during the query must not defeat
            // fresh metadata. A change at/after query start still wins, including
            // the same-second case that timestamps alone cannot order.
            self.archive
                .set_chat_timer(chat, fetched)
                .map_err(|_| storage_error())?;
            current = self.archive.chat_timer(chat).map_err(|_| storage_error())?;
        }
        let mut message = wa::Message::decode_from_slice(raw)
            .map_err(|_| "Could not prepare the message".to_owned())?;
        if !(retry && message.get_ephemeral_expiration().is_some()) {
            // A forwarded message belongs to the destination chat's policy.
            clear_timer(&mut message);
            if let Some(timer) = current.filter(|timer| timer.seconds > 0)
                && !message.set_ephemeral_expiration(timer.seconds)
            {
                return Err("This message type cannot carry the chat's disappearing timer.".into());
            }
        }
        let raw = message.encode_to_vec();
        if !self
            .archive
            .prepare_send_raw(chat, id, &raw)
            .map_err(|_| storage_error())?
        {
            return Err("The outgoing message is no longer pending.".into());
        }
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::super::receipt_tests::{own_message, worker};
    use super::*;

    fn pending(worker: &Worker, chat: &str, id: &str, message: &wa::Message) {
        let mut row = own_message(id, 500);
        row.chat = chat.into();
        row.status = Delivery::Pending;
        worker.archive.ensure_chat(chat, "Fixture").unwrap();
        worker
            .archive
            .insert_message(&row, Some(&message.encode_to_vec()))
            .unwrap();
    }

    fn protocol(seconds: u32, updated_at: i64) -> wa::Message {
        wa::Message {
            protocol_message: MessageField::some(wa::message::ProtocolMessage {
                r#type: Some(wa::message::protocol_message::Type::EPHEMERAL_SETTING),
                ephemeral_expiration: Some(seconds),
                ephemeral_setting_timestamp: Some(updated_at),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn disappearing_timer_reaches_text_and_every_attachment_without_losing_content() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = "123-456@g.us";
        let bodies = [
            wa::Message::text("plain text"),
            wa::Message::text_with_context("quoted text", wa::ContextInfo::default()),
            wa::Message {
                image_message: MessageField::some(wa::message::ImageMessage {
                    caption: Some("photo caption".into()),
                    media_key: Some(vec![7; 32]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            wa::Message {
                video_message: MessageField::some(wa::message::VideoMessage {
                    caption: Some("video caption".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            wa::Message {
                audio_message: MessageField::some(wa::message::AudioMessage {
                    ptt: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            wa::Message {
                document_message: MessageField::some(wa::message::DocumentMessage {
                    file_name: Some("fixture.pdf".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            wa::Message {
                sticker_message: MessageField::some(wa::message::StickerMessage::default()),
                ..Default::default()
            },
        ];
        for (index, mut body) in bodies.into_iter().enumerate() {
            let id = format!("timed-{index}");
            if index != 0 {
                body.set_context_info(wa::ContextInfo {
                    stanza_id: Some("quoted-id".into()),
                    mentioned_jid: vec!["15550002222@s.whatsapp.net".into()],
                    ..Default::default()
                });
            }
            pending(&worker, chat, &id, &body);
            let previous = worker.archive.chat_timer(chat).unwrap();
            let raw = worker
                .prepare_send(
                    chat,
                    &id,
                    &body.encode_to_vec(),
                    previous,
                    Some(timer(86400, 100).unwrap()),
                    false,
                )
                .unwrap();
            let sent = wa::Message::decode_from_slice(&raw).unwrap();
            assert_eq!(sent.get_ephemeral_expiration(), Some(86400));
            assert_eq!(worker.archive.raw(chat, &id).unwrap().unwrap(), raw);
            assert_eq!(sent.text_content(), body.text_content());
            if index != 0 {
                let context = context_of(&sent).unwrap();
                assert_eq!(context.stanza_id.as_deref(), Some("quoted-id"));
                assert_eq!(context.mentioned_jid, ["15550002222@s.whatsapp.net"]);
            }
            if let Some(image) = sent.image_message.as_option() {
                assert_eq!(image.caption.as_deref(), Some("photo caption"));
                assert_eq!(image.media_key.as_deref(), Some([7; 32].as_slice()));
            }
        }
    }

    #[test]
    fn timer_changes_without_message_keys_override_history_and_message_hints() {
        let (mut worker, _events, _inbox, _wa) = worker();
        let chat = own_message("fixture", 0).chat;
        let info = MessageInfo {
            source: MessageSource {
                chat: chat.parse().unwrap(),
                sender: chat.parse().unwrap(),
                ..Default::default()
            },
            ..Default::default()
        };
        worker.ingest(&Arc::new(protocol(86400, 100)), &info);
        assert_eq!(worker.archive.chat_timer(&chat).unwrap(), timer(86400, 100));
        worker.ingest(&Arc::new(protocol(0, 200)), &info);
        let parsed = parse_conversation(wa::Conversation {
            id: chat.clone(),
            ephemeral_expiration: Some(86400),
            ephemeral_setting_timestamp: Some(150),
            ..Default::default()
        });
        worker.apply_history(
            ParsedHistory {
                chats: vec![parsed],
                push_names: vec![],
                lids: vec![],
                stickers: vec![],
            },
            false,
        );
        let hint = wa::Message::text_with_context(
            "from an old client",
            wa::ContextInfo {
                expiration: Some(604800),
                ephemeral_setting_timestamp: Some(300),
                ..Default::default()
            },
        );
        worker.ingest(&Arc::new(hint), &info);
        assert_eq!(worker.archive.chat_timer(&chat).unwrap(), timer(0, 200));
        // A message's timer is not a fresh chat setting once we know the policy.
        assert!(from_message(&wa::Message::text("ordinary"), 400).is_none());
    }

    #[test]
    fn a_phone_change_during_a_group_query_wins_and_retry_keeps_the_sent_timer() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = "123-456@g.us";
        let old = timer(604800, 100).unwrap();
        worker.remember_timer(chat, old);
        let body = wa::Message::text("fixture");
        pending(&worker, chat, "changed", &body);
        worker.remember_timer(chat, timer(0, 200).unwrap());
        let raw = worker
            .prepare_send(
                chat,
                "changed",
                &body.encode_to_vec(),
                Some(old),
                timer(604800, 150),
                false,
            )
            .unwrap();
        assert!(
            wa::Message::decode_from_slice(&raw)
                .unwrap()
                .get_ephemeral_expiration()
                .is_none()
        );

        let mut original = wa::Message::text("retry fixture");
        original.set_ephemeral_expiration(86400);
        pending(&worker, chat, "retry", &original);
        worker
            .archive
            .set_status(chat, "retry", Delivery::Failed, 0)
            .unwrap();
        assert!(worker.archive.retry_failed(chat, "retry").unwrap());
        let raw = worker
            .prepare_send(
                chat,
                "retry",
                &original.encode_to_vec(),
                timer(0, 200),
                timer(0, 300),
                true,
            )
            .unwrap();
        assert_eq!(
            wa::Message::decode_from_slice(&raw)
                .unwrap()
                .get_ephemeral_expiration(),
            Some(86400)
        );
        assert_eq!(
            worker
                .archive
                .message(chat, "retry")
                .unwrap()
                .unwrap()
                .timestamp,
            500
        );
    }

    #[test]
    fn fresh_group_metadata_wins_over_history_received_during_the_query() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = "123-456@g.us";
        let body = wa::Message::text("fixture");
        pending(&worker, chat, "history-race", &body);
        // No setting was known when the query began. An older history snapshot
        // arrives before its reply, but is not a concurrent live setting change.
        worker.remember_timer(chat, timer(604800, 100).unwrap());
        let raw = worker
            .prepare_send(
                chat,
                "history-race",
                &body.encode_to_vec(),
                None,
                timer(0, 200),
                false,
            )
            .unwrap();
        assert_eq!(worker.archive.chat_timer(chat).unwrap(), timer(0, 200));
        assert!(
            wa::Message::decode_from_slice(&raw)
                .unwrap()
                .get_ephemeral_expiration()
                .is_none()
        );
    }

    #[test]
    fn forwards_use_the_destination_timer_and_upgrade_hints_ignore_forwarded_content() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = own_message("fixture", 0).chat;
        let original = wa::Message::text_with_context(
            "forwarded",
            wa::ContextInfo {
                expiration: Some(604800),
                ephemeral_setting_timestamp: Some(100),
                ..Default::default()
            },
        );
        let forwarded = original.prepare_for_forward();
        assert!(from_message(&forwarded, 300).is_none());
        for (index, seconds) in [0, 86400].into_iter().enumerate() {
            worker.remember_timer(&chat, timer(seconds, 400 + index as i64).unwrap());
            pending(&worker, &chat, "forward", &forwarded);
            let raw = worker
                .prepare_send(
                    &chat,
                    "forward",
                    &forwarded.encode_to_vec(),
                    None,
                    None,
                    false,
                )
                .unwrap();
            let sent = wa::Message::decode_from_slice(&raw).unwrap();
            assert_eq!(
                sent.get_ephemeral_expiration(),
                (seconds != 0).then_some(seconds)
            );
            assert!(context_of(&sent).unwrap().is_forwarded.unwrap());
            assert!(
                context_of(&sent)
                    .unwrap()
                    .ephemeral_setting_timestamp
                    .is_none()
            );
        }
        let fresh = "15550003333@s.whatsapp.net";
        let mut incoming = own_message("old-received", 100);
        incoming.chat = fresh.into();
        incoming.from_me = false;
        worker
            .archive
            .insert_message(&incoming, Some(&original.encode_to_vec()))
            .unwrap();
        assert_eq!(worker.outgoing_timer(fresh).unwrap(), timer(604800, 100));
    }

    #[tokio::test]
    async fn queued_send_preparation_rekeys_aliases_and_rejects_an_old_engine() {
        let (mut worker, _events, _inbox, _wa) = worker();
        let lid = "123@lid";
        let pn = "15550004444@s.whatsapp.net";
        let body = wa::Message::text("fixture");
        worker.remember_timer(lid, timer(86400, 100).unwrap());
        pending(&worker, lid, "queued", &body);
        worker.learn_lid("123", "15550004444");
        for (generation, succeeds) in [
            (worker.bot_generation, true),
            (worker.bot_generation + 1, false),
        ] {
            let (prepared, mut ready) = mpsc::unbounded_channel();
            worker
                .handle_command(Command::PrepareSend {
                    chat: lid.into(),
                    id: "queued".into(),
                    generation,
                    raw: body.encode_to_vec(),
                    previous_timer: None,
                    group_timer: None,
                    retry: false,
                    prepared,
                })
                .await;
            let result = ready.recv().await.unwrap();
            assert_eq!(result.is_ok(), succeeds);
            if let Ok(raw) = result {
                assert_eq!(
                    wa::Message::decode_from_slice(&raw)
                        .unwrap()
                        .get_ephemeral_expiration(),
                    Some(86400)
                );
                assert_eq!(worker.archive.raw(pn, "queued").unwrap().unwrap(), raw);
            }
        }
        assert!(worker.archive.raw(lid, "queued").unwrap().is_none());
    }

    #[test]
    fn a_forward_that_fails_before_preparation_retries_with_the_destination_timer() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = "123-456@g.us";
        let mut original = wa::Message::text("fixture");
        original.set_ephemeral_expiration(86400);
        let mut forwarded = original.prepare_for_forward();
        clear_timer(&mut forwarded);
        for (index, seconds) in [0, 604800].into_iter().enumerate() {
            let id = format!("unprepared-forward-{index}");
            pending(&worker, chat, &id, &forwarded);
            worker
                .archive
                .set_status(chat, &id, Delivery::Failed, 0)
                .unwrap();
            assert!(worker.archive.retry_failed(chat, &id).unwrap());
            worker.remember_timer(chat, timer(seconds, 200 + index as i64).unwrap());
            let raw = worker
                .prepare_send(chat, &id, &forwarded.encode_to_vec(), None, None, true)
                .unwrap();
            let sent = wa::Message::decode_from_slice(&raw).unwrap();
            assert_eq!(
                sent.get_ephemeral_expiration(),
                (seconds > 0).then_some(seconds)
            );
            assert!(forwarded_of(&sent));
        }
    }

    #[test]
    fn incomplete_group_settings_cannot_silently_disable_the_timer() {
        assert_eq!(group_duration(None).unwrap(), 0);
        assert!(group_duration(Some(whatsapp_rust::GroupEphemeralSettings::default())).is_err());
        assert!(timer(u32::MAX, 100).is_none());
        assert!(timer(86400, -1).is_none());
    }

    #[test]
    fn forwarding_video_notes_locations_contact_lists_and_polls_clears_the_old_timer() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = own_message("fixture", 0).chat;
        worker.remember_timer(&chat, timer(0, 200).unwrap());
        let bodies = [
            wa::Message {
                ptv_message: MessageField::some(Default::default()),
                ..Default::default()
            },
            wa::Message {
                live_location_message: MessageField::some(Default::default()),
                ..Default::default()
            },
            wa::Message {
                contacts_array_message: MessageField::some(Default::default()),
                ..Default::default()
            },
            wa::Message {
                poll_creation_message_v3: MessageField::some(Default::default()),
                ..Default::default()
            },
        ];
        for (index, mut body) in bodies.into_iter().enumerate() {
            body.set_ephemeral_expiration(86400);
            let forwarded = body.prepare_for_forward();
            let id = format!("forward-{index}");
            pending(&worker, &chat, &id, &forwarded);
            let raw = worker
                .prepare_send(&chat, &id, &forwarded.encode_to_vec(), None, None, false)
                .unwrap();
            let sent = wa::Message::decode_from_slice(&raw).unwrap();
            assert!(sent.get_ephemeral_expiration().is_none());
            assert!(forwarded_of(&sent));
        }
    }

    #[test]
    fn upgrade_does_not_reenable_an_older_timer_past_a_newer_untimed_message() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = own_message("fixture", 0).chat;
        for (id, timestamp, seconds) in [("timed", 100, Some(86400)), ("untimed", 200, None)] {
            let mut row = own_message(id, timestamp);
            row.from_me = false;
            let body = wa::Message::text_with_context(
                "fixture",
                wa::ContextInfo {
                    expiration: seconds,
                    ..Default::default()
                },
            );
            worker
                .archive
                .insert_message(&row, Some(&body.encode_to_vec()))
                .unwrap();
        }
        assert!(worker.outgoing_timer(&chat).unwrap().is_none());
    }

    #[test]
    fn upgrade_reads_one_bounded_body_and_does_not_skip_a_large_newer_message() {
        let (worker, _events, _inbox, _wa) = worker();
        let chat = own_message("fixture", 0).chat;
        let mut timed = wa::Message::text("fixture");
        timed.set_ephemeral_expiration(86400);
        for (id, timestamp, forwarded, raw) in [
            ("timed", 100, false, timed.encode_to_vec()),
            (
                "forward",
                200,
                true,
                timed.prepare_for_forward().encode_to_vec(),
            ),
        ] {
            let mut row = own_message(id, timestamp);
            row.from_me = false;
            row.forwarded = forwarded;
            worker.archive.insert_message(&row, Some(&raw)).unwrap();
        }
        assert_eq!(
            worker.archive.latest_timer_source(&chat).unwrap(),
            Some((100, Some(timed.encode_to_vec())))
        );
        let mut row = own_message("large", 300);
        row.from_me = false;
        worker
            .archive
            .insert_message(&row, Some(&vec![0; 262145]))
            .unwrap();
        assert_eq!(
            worker.archive.latest_timer_source(&chat).unwrap(),
            Some((300, None))
        );
        assert!(worker.outgoing_timer(&chat).unwrap().is_none());
    }

    #[tokio::test]
    async fn live_group_notifications_persist_timer_changes() {
        let (mut worker, _events, _inbox, _wa) = worker();
        let chat = "123-456@g.us";
        for (seconds, updated_at) in [(86400, 100), (0, 200)] {
            let update = wa_events::GroupUpdate::builder()
                .group_jid(chat.parse().unwrap())
                .timestamp((std::time::UNIX_EPOCH + Duration::from_secs(updated_at as u64)).into())
                .is_lid_addressing_mode(false)
                .action(Box::new(
                    whatsapp_rust::wacore::stanza::groups::GroupNotificationAction::Ephemeral {
                        expiration: seconds,
                        trigger: None,
                    },
                ))
                .build();
            worker
                .handle_wa_event(Arc::new(wa_events::Event::GroupUpdate(update)))
                .await;
            assert_eq!(
                worker.archive.chat_timer(chat).unwrap(),
                timer(seconds, updated_at)
            );
        }
    }
}
