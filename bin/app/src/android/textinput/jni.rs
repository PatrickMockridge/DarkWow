/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use miniquad::native::android::ndk_sys;

use super::gametextinput::{GameTextInput, GAME_TEXT_INPUT};

/// Set the InputConnection for GameTextInput (called from Java)
///
/// This follows the official Android GameTextInput integration pattern:
/// https://developer.android.com/games/agdk/add-support-for-text-input
///
/// Called from MainActivity when the InputConnection is created. It passes
/// the Java InputConnection object to the native GameTextInput library.
#[no_mangle]
pub extern "C" fn Java_dwow_dwow_1app_MainActivity_setInputConnectionNative(
    _env: *mut ndk_sys::JNIEnv,
    _class: ndk_sys::jclass,
    input_connection: ndk_sys::jobject,
) {
    debug!(target: "android::textinput::jni", "Setting input connection");
    // Initialize GameTextInput on first call
    let gti = GAME_TEXT_INPUT.get_or_init(|| GameTextInput::new());
    gti.set_input_connection(input_connection);
}

/// Process IME state event from Java Listener.stateChanged()
///
/// This follows the official Android GameTextInput integration pattern.
/// Called from the Java InputConnection's Listener whenever the IME sends
/// a state change (text typed, cursor moved, etc.).
#[no_mangle]
pub extern "C" fn Java_dwow_dwow_1app_MainActivity_onTextInputEventNative(
    _env: *mut ndk_sys::JNIEnv,
    _class: ndk_sys::jclass,
    soft_keyboard_event: ndk_sys::jobject,
) {
    let gti = GAME_TEXT_INPUT.get().unwrap();
    gti.process_event(soft_keyboard_event);
}
