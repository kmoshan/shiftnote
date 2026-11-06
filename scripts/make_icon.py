"""生成 ShiftNote 应用图标（1024x1024 PNG）。

设计：深蓝圆角方块 + 白色交接清单卡片 + P0 橙红圆点，保证在 32px 下仍可辨识。
"""

from PIL import Image, ImageDraw

SIZE = 1024
SS = 4  # 超采样倍数，先画大图再缩小，得到平滑边缘
W = SIZE * SS

BG_TOP = (47, 111, 235)
BG_BOTTOM = (31, 78, 194)
CARD = (255, 255, 255)
LINE = (47, 111, 235, 110)
ACCENT = (233, 106, 45)


def rounded_rect_mask(size, radius):
    mask = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(mask)
    d.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def main():
    # 渐变背景
    bg = Image.new("RGB", (W, W))
    d = ImageDraw.Draw(bg)
    for y in range(W):
        t = y / (W - 1)
        d.line(
            [(0, y), (W, y)],
            fill=(
                int(BG_TOP[0] + (BG_BOTTOM[0] - BG_TOP[0]) * t),
                int(BG_TOP[1] + (BG_BOTTOM[1] - BG_TOP[1]) * t),
                int(BG_TOP[2] + (BG_BOTTOM[2] - BG_TOP[2]) * t),
            ),
        )

    canvas = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    canvas.paste(bg, (0, 0), rounded_rect_mask(W, int(W * 0.22)))

    d = ImageDraw.Draw(canvas)

    # 清单卡片
    card_w = int(W * 0.52)
    card_h = int(W * 0.60)
    card_x = (W - card_w) // 2
    card_y = int(W * 0.20)
    d.rounded_rectangle(
        (card_x, card_y, card_x + card_w, card_y + card_h),
        radius=int(W * 0.055),
        fill=CARD,
    )

    # 卡片顶部夹子
    tab_w = int(card_w * 0.34)
    tab_h = int(W * 0.045)
    d.rounded_rectangle(
        (
            (W - tab_w) // 2,
            card_y - tab_h // 2,
            (W + tab_w) // 2,
            card_y + tab_h // 2,
        ),
        radius=tab_h // 2,
        fill=ACCENT,
    )

    # 三条清单项：勾选框 + 文本条
    row_h = int(W * 0.036)
    gap = int(W * 0.075)
    start_y = card_y + int(card_h * 0.26)
    for i in range(3):
        cy = start_y + i * gap
        box = int(W * 0.052)
        bx = card_x + int(card_w * 0.10)
        d.rounded_rectangle(
            (bx, cy, bx + box, cy + box), radius=int(box * 0.28), fill=(232, 238, 250)
        )
        # 前两项打勾
        if i < 2:
            d.line(
                [
                    (bx + box * 0.22, cy + box * 0.52),
                    (bx + box * 0.43, cy + box * 0.74),
                    (bx + box * 0.80, cy + box * 0.28),
                ],
                fill=ACCENT if i == 0 else (47, 111, 235),
                width=int(W * 0.011),
                joint="curve",
            )
        line_x = bx + box + int(card_w * 0.08)
        line_w = card_w - (line_x - card_x) - int(card_w * 0.16)
        d.rounded_rectangle(
            (line_x, cy + box * 0.34, line_x + line_w, cy + box * 0.34 + row_h * 0.5),
            radius=row_h // 4,
            fill=(206, 214, 226),
        )

    canvas = canvas.resize((SIZE, SIZE), Image.LANCZOS)
    out = "src-tauri/icons/source-icon.png"
    canvas.save(out, "PNG")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
