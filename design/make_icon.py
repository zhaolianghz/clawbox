from PIL import Image, ImageDraw, ImageFilter
import math

S = 4096            # 超采样画布
OUT = 1024
img = Image.new('RGBA', (S, S), (0, 0, 0, 0))

# ---------- 圆角方形底(macOS 风格,占 82%,留透明边距) ----------
pad = int(S * 0.09)
box = [pad, pad, S - pad, S - pad]
radius = int(S * 0.185)

bg = Image.new('RGBA', (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(bg)
d.rounded_rectangle(box, radius=radius, fill=(15, 23, 42, 255))  # #0f172a

# 底部到顶部的微渐变(上浅下深)
grad = Image.new('L', (1, S))
for y in range(S):
    t = y / S
    grad.putpixel((0, y), int(38 * (1 - t)))
grad = grad.resize((S, S))
tint = Image.new('RGBA', (S, S), (56, 189, 248, 0))
tint.putalpha(grad)
mask = Image.new('L', (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle(box, radius=radius, fill=255)
glow_top = Image.composite(Image.new('RGBA', (S, S), (30, 58, 92, 255)), Image.new('RGBA', (S, S), (0,0,0,0)), grad)
bg = Image.alpha_composite(bg, Image.composite(glow_top, Image.new('RGBA',(S,S),(0,0,0,0)), mask))
img = Image.alpha_composite(img, bg)

# ---------- 爪痕:三道弧形渐细划痕 ----------
def slash(cx0, cy0, cx1, cy1, bend, w):
    """二次贝塞尔中心线 + 两端收尖的厚度轮廓 -> 多边形点集"""
    mx, my = (cx0+cx1)/2, (cy0+cy1)/2
    dx, dy = cx1-cx0, cy1-cy0
    L = math.hypot(dx, dy)
    nx, ny = -dy/L, dx/L                      # 法向
    px, py = mx + nx*bend, my + ny*bend       # 控制点
    N = 80
    top, bot = [], []
    for i in range(N+1):
        t = i / N
        x = (1-t)**2*cx0 + 2*(1-t)*t*px + t**2*cx1
        y = (1-t)**2*cy0 + 2*(1-t)*t*py + t**2*cy1
        tx = 2*(1-t)*(px-cx0) + 2*t*(cx1-px)
        ty = 2*(1-t)*(py-cy0) + 2*t*(cy1-py)
        tl = math.hypot(tx, ty) or 1
        nxx, nyy = -ty/tl, tx/tl
        ww = w * math.sin(math.pi * t) ** 0.75   # 两端收尖
        top.append((x + nxx*ww, y + nyy*ww))
        bot.append((x - nxx*ww, y - nyy*ww))
    return top + bot[::-1]

claw = Image.new('L', (S, S), 0)
cd = ImageDraw.Draw(claw)
cx, cy = S/2, S/2
ang = math.radians(-52)                        # 整体倾斜
for i, (off, ln, w) in enumerate([(-0.155, 0.235, 0.046), (0.0, 0.29, 0.052), (0.155, 0.235, 0.046)]):
    ox, oy = off*S*math.cos(ang+math.pi/2), off*S*math.sin(ang+math.pi/2)
    hx, hy = ln*S*math.cos(ang), ln*S*math.sin(ang)
    pts = slash(cx-hx+ox, cy-hy+oy, cx+hx+ox, cy+hy+oy, S*0.055, S*w)
    cd.polygon(pts, fill=255)

# 爪痕渐变填充(青 -> 蓝)
gfill = Image.new('RGBA', (S, S))
gd = ImageDraw.Draw(gfill)
for y in range(0, S, 8):
    t = y / S
    r = int(34 + (14-34)*t); g = int(211 + (165-211)*t); b = int(238 + (233-238)*t)
    gd.rectangle([0, y, S, y+8], fill=(r, g, b, 255))

# 外发光
glow = claw.filter(ImageFilter.GaussianBlur(S*0.02))
glow_layer = Image.new('RGBA', (S, S), (34, 211, 238, 0))
glow_layer.putalpha(glow.point(lambda v: int(v*0.55)))
img = Image.alpha_composite(img, Image.composite(glow_layer, Image.new('RGBA',(S,S),(0,0,0,0)), mask))

# 爪痕本体(裁在圆角内)
claw_in = Image.new('L', (S, S), 0)
claw_in.paste(claw, (0, 0), mask)
img.paste(gfill, (0, 0), claw_in)

img = img.resize((OUT, OUT), Image.LANCZOS)
img.save('design/icon-1024.png')
print('saved 1024x1024')

# 用法:python3 design/make_icon.py 生成 1024px 源图后,执行
#   npx tauri icon design/icon-1024.png
# 重新生成 src-tauri/icons/ 全套(macOS icns / Windows ico / Linux png / Android / iOS)
