#!/usr/bin/env python3
"""
Gera o ícone oficial do FastFile em alta resolução (PNG e ICO multi-resolução).
"""

import math
from PIL import Image, ImageDraw, ImageFilter

SIZE = 512

def create_fastfile_icon(size=512):
    # Cria imagem RGBA com fundo transparente
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    scale = size / 512.0

    # 1. Base Squircle / Cantos Arredondados
    margin = int(24 * scale)
    corner_radius = int(100 * scale)
    bbox = [margin, margin, size - margin, size - margin]

    # Preenche o fundo com gradiente sutil escuro
    # Criamos máscara para aplicar o gradiente vertical
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(bbox, radius=corner_radius, fill=255)

    bg_grad = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg_draw = ImageDraw.Draw(bg_grad)
    for y in range(margin, size - margin):
        ratio = (y - margin) / (size - 2 * margin)
        # De #10121d para #1b1e2e
        r = int(16 + ratio * 12)
        g = int(18 + ratio * 14)
        b = int(29 + ratio * 20)
        bg_draw.line([(margin, y), (size - margin, y)], fill=(r, g, b, 255))

    img.paste(bg_grad, (0, 0), mask)

    # Borda externa com gradiente índigo (#6366f1)
    border_width = max(1, int(5 * scale))
    draw.rounded_rectangle(bbox, radius=corner_radius, outline=(99, 102, 241, 190), width=border_width)

    # 2. Silhueta de Folha de Documento / Arquivo
    doc_x0 = int(145 * scale)
    doc_y0 = int(75 * scale)
    doc_x1 = int(365 * scale)
    doc_y1 = int(435 * scale)
    fold_size = int(60 * scale)

    # Pontos do polígono da folha com corte chanfrado
    doc_poly = [
        (doc_x0, doc_y0),
        (doc_x1 - fold_size, doc_y0),
        (doc_x1, doc_y0 + fold_size),
        (doc_x1, doc_y1),
        (doc_x0, doc_y1),
    ]

    doc_mask = Image.new("L", (size, size), 0)
    doc_mask_draw = ImageDraw.Draw(doc_mask)
    doc_mask_draw.polygon(doc_poly, fill=255)

    doc_grad = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    doc_draw = ImageDraw.Draw(doc_grad)
    for y in range(doc_y0, doc_y1):
        ratio = (y - doc_y0) / (doc_y1 - doc_y0)
        # De #252b44 para #181b2c
        r = int(37 - ratio * 12)
        g = int(43 - ratio * 15)
        b = int(68 - ratio * 24)
        doc_draw.line([(doc_x0, y), (doc_x1, y)], fill=(r, g, b, 255))

    img.paste(doc_grad, (0, 0), doc_mask)

    # Contorno do documento
    draw.polygon(doc_poly, outline=(55, 65, 95, 230), width=max(1, int(2.5 * scale)))

    # Dobra superior direita da folha
    fold_poly = [
        (doc_x1 - fold_size, doc_y0),
        (doc_x1 - fold_size, doc_y0 + fold_size),
        (doc_x1, doc_y0 + fold_size),
    ]
    draw.polygon(fold_poly, fill=(67, 56, 202, 255), outline=(99, 102, 241, 255), width=max(1, int(2 * scale)))

    # Linhas de código / texto sutis dentro do documento
    line_y_starts = [int(160 * scale), int(190 * scale), int(220 * scale)]
    line_widths = [int(80 * scale), int(120 * scale), int(90 * scale)]
    for ly, lw in zip(line_y_starts, line_widths):
        draw.rounded_rectangle(
            [doc_x0 + int(30 * scale), ly, doc_x0 + int(30 * scale) + lw, ly + int(8 * scale)],
            radius=int(4 * scale),
            fill=(71, 85, 105, 120),
        )

    # 3. Anel de Busca & Velocidade (Lens Ring)
    center_x = int(256 * scale)
    center_y = int(270 * scale)
    ring_radius = int(85 * scale)
    draw.ellipse(
        [center_x - ring_radius, center_y - ring_radius, center_x + ring_radius, center_y + ring_radius],
        outline=(56, 189, 248, 80),
        width=max(1, int(2 * scale)),
    )

    # 4. Raio Elétrico Central (Lightning Bolt - FastFile Símbolo)
    bolt_pts = [
        (int(288 * scale), int(100 * scale)),
        (int(185 * scale), int(262 * scale)),
        (int(258 * scale), int(262 * scale)),
        (int(228 * scale), int(418 * scale)),
        (int(335 * scale), int(250 * scale)),
        (int(262 * scale), int(250 * scale)),
    ]

    # Cria máscara do raio para preencher com gradiente ciano -> azul -> púrpura neon
    bolt_mask = Image.new("L", (size, size), 0)
    bolt_mask_draw = ImageDraw.Draw(bolt_mask)
    bolt_mask_draw.polygon(bolt_pts, fill=255)

    bolt_grad = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bolt_draw = ImageDraw.Draw(bolt_grad)
    bolt_min_y = int(100 * scale)
    bolt_max_y = int(418 * scale)
    for y in range(bolt_min_y, bolt_max_y + 1):
        ratio = (y - bolt_min_y) / (bolt_max_y - bolt_min_y)
        if ratio < 0.4:
            r1 = ratio / 0.4
            r = int(56 + r1 * (99 - 56))
            g = int(189 - r1 * (189 - 102))
            b = int(248 - r1 * (248 - 241))
        elif ratio < 0.75:
            r2 = (ratio - 0.4) / 0.35
            r = int(99 + r2 * (139 - 99))
            g = int(102 - r2 * (102 - 92))
            b = int(241 + r2 * (246 - 241))
        else:
            r3 = (ratio - 0.75) / 0.25
            r = int(139 + r3 * (217 - 139))
            g = int(92 - r3 * (92 - 70))
            b = int(246 - r3 * (246 - 239))
        bolt_draw.line([(0, y), (size, y)], fill=(r, g, b, 255))

    img.paste(bolt_grad, (0, 0), bolt_mask)

    # Brilho central / specular highlight no raio
    inner_pts = [
        (int(282 * scale), int(120 * scale)),
        (int(202 * scale), int(258 * scale)),
        (int(255 * scale), int(258 * scale)),
        (int(235 * scale), int(390 * scale)),
        (int(318 * scale), int(254 * scale)),
        (int(265 * scale), int(254 * scale)),
    ]
    inner_mask = Image.new("L", (size, size), 0)
    inner_mask_draw = ImageDraw.Draw(inner_mask)
    inner_mask_draw.polygon(inner_pts, fill=255)

    inner_highlight = Image.new("RGBA", (size, size), (255, 255, 255, 95))
    img.paste(inner_highlight, (0, 0), inner_mask)

    return img

def main():
    print("Gerando ícones do FastFile...")
    img512 = create_fastfile_icon(512)
    img256 = img512.resize((256, 256), Image.Resampling.LANCZOS)
    img128 = img512.resize((128, 128), Image.Resampling.LANCZOS)
    img64 = img512.resize((64, 64), Image.Resampling.LANCZOS)
    img48 = img512.resize((48, 48), Image.Resampling.LANCZOS)
    img32 = img512.resize((32, 32), Image.Resampling.LANCZOS)
    img16 = img512.resize((16, 16), Image.Resampling.LANCZOS)

    # Salva PNGs
    img512.save("assets/fastfile-icon-512.png", "PNG")
    img256.save("assets/fastfile-icon.png", "PNG")
    img256.save("crates/ff-ui/icon.png", "PNG")
    print("✓ Salvo assets/fastfile-icon.png")

    # Salva ICO multi-resolução para Windows
    ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    img512.save(
        "assets/fastfile.ico",
        format="ICO",
        sizes=ico_sizes
    )
    img512.save(
        "crates/ff-ui/fastfile.ico",
        format="ICO",
        sizes=ico_sizes
    )
    print("✓ Salvo assets/fastfile.ico e crates/ff-ui/fastfile.ico")

if __name__ == "__main__":
    main()
