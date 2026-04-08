#![allow(dead_code)]

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::LoginAccountResponse;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use std::sync::Arc;
use std::sync::RwLock;

use crate::shimmer::shimmer_spans;
use crate::tui::FrameRequester;

use super::AuthModeWidget;
use super::ContinueWithDeviceCodeState;
use super::SignInState;
use super::login_error_message;
use super::mark_url_hyperlink;
use super::onboarding_request_id;
use super::unexpected_login_response_error;

pub(super) fn start_headless_chatgpt_login(widget: &mut AuthModeWidget) {
    let request_id = onboarding_request_id();
    let request_id_text = request_id.to_string();

    *widget.error.write().unwrap() = None;
    *widget.sign_in_state.write().unwrap() = SignInState::ChatGptDeviceCode(
        ContinueWithDeviceCodeState::pending(request_id_text.clone()),
    );
    widget.request_frame.schedule_frame();

    let sign_in_state = widget.sign_in_state.clone();
    let request_frame = widget.request_frame.clone();
    let error = widget.error.clone();
    let request_handle = widget.app_server_request_handle.clone();

    tokio::spawn(async move {
        let result = request_handle
            .request_typed::<LoginAccountResponse>(ClientRequest::LoginAccount {
                request_id,
                params: LoginAccountParams::ChatgptDeviceCode,
            })
            .await
            .map_err(|err| login_error_message("启动设备码登录失败", err));
        apply_device_code_login_response_for_active_request(
            &sign_in_state,
            &request_frame,
            &error,
            &request_id_text,
            result,
        );
    });
}

pub(super) fn render_device_code_login(
    widget: &AuthModeWidget,
    area: Rect,
    buf: &mut Buffer,
    state: &ContinueWithDeviceCodeState,
) {
    let banner = if state.is_showing_copyable_auth() {
        "请在浏览器中完成登录"
    } else {
        "正在准备设备码登录"
    };

    let mut spans = vec!["  ".into()];
    if widget.animations_enabled {
        widget
            .request_frame
            .schedule_frame_in(std::time::Duration::from_millis(100));
        spans.extend(shimmer_spans(banner));
    } else {
        spans.push(banner.into());
    }

    let mut lines = vec![spans.into(), "".into()];
    let verification_url =
        if let (Some(url), Some(user_code)) = (&state.verification_url, &state.user_code) {
            lines.push("  1. 在浏览器中打开此链接并登录".into());
            lines.push("".into());
            lines.push(Line::from(vec![
                "  ".into(),
                url.as_str().cyan().underlined(),
            ]));
            lines.push("".into());
            lines.push("  2. 登录后输入此一次性验证码（15 分钟后过期）".into());
            lines.push("".into());
            lines.push(Line::from(vec![
                "  ".into(),
                user_code.as_str().cyan().bold(),
            ]));
            lines.push("".into());
            lines.push(
                "  设备码是常见的钓鱼目标，请勿向他人分享此验证码。"
                    .dim()
                    .into(),
            );
            lines.push("".into());
            Some(url.clone())
        } else {
            lines.push("  正在请求一次性验证码...".dim().into());
            lines.push("".into());
            None
        };

    lines.push("  按 Esc 取消".dim().into());
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);

    if let Some(url) = &verification_url {
        mark_url_hyperlink(buf, area, url);
    }
}

fn device_code_request_matches(state: &SignInState, request_id: &str) -> bool {
    matches!(
        state,
        SignInState::ChatGptDeviceCode(state) if state.request_id == request_id
    )
}

fn set_device_code_state_for_active_request(
    sign_in_state: &Arc<RwLock<SignInState>>,
    request_frame: &FrameRequester,
    request_id: &str,
    next_state: SignInState,
) -> bool {
    let mut guard = sign_in_state.write().unwrap();
    if !device_code_request_matches(&guard, request_id) {
        return false;
    }

    *guard = next_state;
    drop(guard);
    request_frame.schedule_frame();
    true
}

fn set_device_code_error_for_active_request(
    sign_in_state: &Arc<RwLock<SignInState>>,
    request_frame: &FrameRequester,
    error: &Arc<RwLock<Option<String>>>,
    request_id: &str,
    message: String,
) -> bool {
    if !set_device_code_state_for_active_request(
        sign_in_state,
        request_frame,
        request_id,
        SignInState::PickMode,
    ) {
        return false;
    }

    *error.write().unwrap() = Some(message);
    request_frame.schedule_frame();
    true
}

fn apply_device_code_login_response_for_active_request(
    sign_in_state: &Arc<RwLock<SignInState>>,
    request_frame: &FrameRequester,
    error: &Arc<RwLock<Option<String>>>,
    request_id: &str,
    result: Result<LoginAccountResponse, String>,
) {
    match result {
        Ok(LoginAccountResponse::ChatgptDeviceCode {
            login_id,
            verification_url,
            user_code,
        }) => {
            *error.write().unwrap() = None;
            let _ = set_device_code_state_for_active_request(
                sign_in_state,
                request_frame,
                request_id,
                SignInState::ChatGptDeviceCode(ContinueWithDeviceCodeState::ready(
                    request_id.to_string(),
                    login_id,
                    verification_url,
                    user_code,
                )),
            );
        }
        Ok(other) => {
            set_device_code_error_for_active_request(
                sign_in_state,
                request_frame,
                error,
                request_id,
                unexpected_login_response_error(&other),
            );
        }
        Err(err) => {
            set_device_code_error_for_active_request(
                sign_in_state,
                request_frame,
                error,
                request_id,
                err,
            );
        }
    }
}
