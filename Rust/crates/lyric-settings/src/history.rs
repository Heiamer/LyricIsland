//! History page: renders `changelog::entries()` as cards. No parsing or tests live here, so the
//! pure logic stays independent of the UI.
use crate::changelog;
use crate::theme as t;
use gpui::{prelude::*, *};

pub fn rows() -> Vec<AnyElement> {
    let mut rows = vec![card(
        "歌词岛历次更新",
        "0.2.0 独立版 → 当前开发版",
        "按版本倒序展示；记录描述当时版本行为。0.7.9 为体积优化分支，0.8 为未正式发布的开发快照。现存最早独立版资料为 0.2.0。",
    )];
    rows.extend(changelog::entries().iter().map(|entry| card(&entry.title, &entry.date, &entry.body)));
    rows
}

fn card(title: &str, date: &str, body: &str) -> AnyElement {
    div().px_4().py_3().rounded(px(12.0)).bg(t::surface()).border_1().border_color(t::hairline())
        .child(
            div().flex().flex_row().items_center().justify_between().gap_4()
                .child(div().text_size(px(13.5)).text_color(t::text()).child(title.to_owned()))
                .child(div().text_size(px(11.0)).text_color(t::text_faint()).child(date.to_owned())),
        )
        .child(div().mt_2().text_size(px(12.0)).text_color(t::text_dim()).child(body.to_owned()))
        .into_any_element()
}
