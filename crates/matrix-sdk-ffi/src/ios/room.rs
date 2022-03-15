use super::messages::{Message, sync_event_to_message};
use super::backward_stream::BackwardsStream;
use super::RUNTIME;

use anyhow::{Result};
use parking_lot::RwLock;
use std::sync::Arc;
use futures::{pin_mut, StreamExt};
use matrix_sdk::media::MediaFormat;
use matrix_sdk::{
    room::Room as MatrixRoom,
};


pub trait RoomDelegate: Sync + Send {
    fn did_receive_message(&self, messages: Arc<Message>);
}

pub struct Room {
    room: MatrixRoom,
    delegate: Arc<RwLock<Option<Box<dyn RoomDelegate>>>>,
    is_listening_to_live_events: Arc<RwLock<bool>>
}

impl Room {
    pub fn new(room: MatrixRoom) -> Self {
        Room {
            room,
            delegate: Arc::new(RwLock::new(None)),
            is_listening_to_live_events: Arc::new(RwLock::new(false))
        }
    }

    pub fn set_delegate(&self, delegate: Option<Box<dyn RoomDelegate>>) {
        *self.delegate.write() = delegate;
    }

    pub fn id(&self) -> String {
        self.room.room_id().to_string()
    }

    pub fn name(&self) -> Option<String> {
        self.room.name()
    }

    pub fn display_name(&self) -> Result<String> {
        let r = self.room.clone();
        RUNTIME.block_on(async move {
            Ok(r.display_name().await?)
        })
    }

    pub fn topic(&self) -> Option<String> {
        self.room.topic()
    }

    pub fn avatar(&self) -> Result<Vec<u8>> {
        let r = self.room.clone();
        RUNTIME.block_on(async move {
            Ok(r.avatar(MediaFormat::File).await?.expect("No avatar"))
        })
    }

    pub fn avatar_url(&self) -> Option<String> {
        self.room.avatar_url().map(|m| m.to_string())
    }

    pub fn is_direct(&self) -> bool {
        self.room.is_direct()
    }

    pub fn is_public(&self) -> bool {
        self.room.is_public()
    }

    pub fn is_encrypted(&self) -> bool {
        self.room.is_encrypted()
    }

    pub fn is_space(&self) -> bool {
        self.room.is_space()
    }

    pub fn start_live_event_listener(&self) -> Option<Arc<BackwardsStream>> {
        if *self.is_listening_to_live_events.read() == true {
            return None
        }

        *self.is_listening_to_live_events.write() = true;

        let room = self.room.clone();
        let delegate = self.delegate.clone();
        let is_listening_to_live_events = self.is_listening_to_live_events.clone();

        let (forward_stream, backwards) = RUNTIME.block_on(async move {
            room.timeline().await.expect("Failed acquiring timeline streams")
        });

        RUNTIME.spawn(async move {
            pin_mut!(forward_stream);
            
            while let Some(sync_event) = forward_stream.next().await {
                if *is_listening_to_live_events.read() == false {
                    return
                }

                if let Some(delegate) = &*delegate.read() {
                    if let Some(message) = sync_event_to_message(sync_event) {
                        delegate.did_receive_message(message)
                    }
                }
            }
        });
        Some(Arc::new(BackwardsStream::new(Box::pin(backwards))))
    }

    pub fn stop_live_event_listener(&self) {
        *self.is_listening_to_live_events.write() = false;
    }   
}

impl std::ops::Deref for Room {
    type Target = MatrixRoom;
    fn deref(&self) -> &MatrixRoom {
        &self.room
    }
}