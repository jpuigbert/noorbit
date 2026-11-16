#!/usr/bin/env python3
"""
NoOrbit · spot — banda sonora + mescla final.

  1. Sintetitza la música (cap mostra, cap llicència).
     Instrumentació: caixa de música, pad de corda, baix, maller, shaker,
     campanes i un contrapunt de notes brillants.
  2. ffmpeg li aplica reverberació, EQ i compressió.
  3. Es mescla amb la locució i es fa el ducking (la música baixa quan parla).

Idea musical: 96 BPM, Do major, progressió | C | Am | F | G |.
El "tema" del vídeo és la farg: còlid, fet a mà, esperançat. Per això la
tessitura és de caixa de música i no de sintetitzador fred, i per això el
dialogue es fa sobre un llit(thin) i la música s'obre quan passa l'acció.

Ús:  python3 build/audio.py
"""
import array
import json
import math
import os
import struct
import subprocess
import sys

ROOT = "/Users/appleone/programes/noorbit/promo"
BUILD = os.path.join(ROOT, "build")
VO_DIR = os.path.join(BUILD, "vo")
AUDIO = os.path.join(BUILD, "audio")
FF = os.path.join(ROOT, "node_modules/ffmpeg-static/ffmpeg")
os.makedirs(AUDIO, exist_ok=True)

SR = 48000
DUR = 65.0
N = int(SR * DUR)
BPM = 96.0
BEAT = 60.0 / BPM
BAR = BEAT * 4

# ── notes ───────────────────────────────────────────────────────────────────
PITCH = {
    "A1": 55.00, "B1": 61.74,
    "C2": 65.41, "D2": 73.42, "E2": 82.41, "F2": 87.31, "G2": 98.00, "A2": 110.00, "B2": 123.47,
    "C3": 130.81, "D3": 146.83, "E3": 164.81, "F3": 174.61, "G3": 196.00, "A3": 220.00, "B3": 246.94,
    "C4": 261.63, "D4": 293.66, "E4": 329.63, "F4": 349.23, "G4": 392.00, "A4": 440.00, "B4": 493.88,
    "C5": 523.25, "D5": 587.33, "E5": 659.25, "F5": 698.46, "G5": 783.99, "A5": 880.00, "B5": 987.77,
    "C6": 1046.50, "D6": 1174.66, "E6": 1318.51, "G6": 1567.98, "C7": 2093.00, "E7": 2637.02,
}
# una octava per damunt
PITCH_OCT = dict(PITCH)
for k in list(PITCH):
    PITCH_OCT[k + "+"] = PITCH[k] * 2

# progressió: un acord per temps (2,5 s)
CHORDS = [
    {"root": "C2", "pad": ["C4", "E4", "G4"], "meld": ["E5", "D5", "C5", "G4"]},   # C
    {"root": "A1", "pad": ["A3", "C4", "E4"], "meld": ["A4", "C5", "E5", "D5"]},   # Am
    {"root": "F2", "pad": ["F3", "A3", "C4"], "meld": ["C5", "A4", "F4", "F4"]},    # F
    {"root": "G2", "pad": ["G3", "B3", "D4"], "meld": ["G4", "B4", "D5", "G4"]},    # G
]


def buf():
    return array.array("f", bytes(4 * N))


def add(buf, start, dur, fn, amp=1.0, attack=0.005, release=0.0, pan=0.0):
    i0 = max(0, int(start * SR))
    i1 = min(N, int((start + dur) * SR))
    n = i1 - i0
    if n <= 0:
        return
    a = max(1, int(attack * SR))
    rl = int(release * SR)
    for i in range(n):
        v = fn(i / SR, i)          # <- el primer argument és TEMPS, no l'índex
        if i < a:
            v *= i / a
        if rl and i > n - rl:
            v *= (n - i) / rl
        j = i0 + i
        buf[j] += v * amp


def musicbox(freq, dur=1.6, vib=0.0):
    """caixa de música / celesta: atac ràpid, cua llarga, un pèl de vibrato."""
    f1, f2, f3 = freq, freq * 2.0, freq * 3.01
    decay = 3.0 / max(0.35, dur)
    def fn(t, i):
        d = math.exp(-decay * t)
        v = math.sin(2 * math.pi * f1 * t) * d
        v += 0.30 * math.sin(2 * math.pi * f2 * t) * math.exp(-decay * 1.7 * t)
        v += 0.10 * math.sin(2 * math.pi * f3 * t) * math.exp(-decay * 2.4 * t)
        if vib:
            v *= 1.0 + vib * math.sin(2 * math.pi * 5.2 * t)
        return v
    return fn


def padvoice(freq, dur):
    """sintetitzador de corda: serrellos d'ona saw suaus."""
    f1, f2 = freq, freq * 1.004
    def fn(t, i):
        v = 0.0
        for k in range(1, 7):
            a = 1.0 / k
            v += a * math.sin(2 * math.pi * f1 * k * t)
            v += 0.7 * a * math.sin(2 * math.pi * f2 * k * t)
        env = min(1.0, t / 0.6) * min(1.0, max(0.0, (dur - t) / 1.5))
        return v * 0.17 * env
    return fn


def bassnote(freq, dur):
    def fn(t, i):
        v = math.sin(2 * math.pi * freq * t) + 0.22 * math.sin(2 * math.pi * freq * 2 * t)
        return v * math.exp(-1.5 * t) * min(1.0, t / 0.02)
    return fn


def mallet(freq, dur=0.45):
    def fn(t, i):
        v = math.sin(2 * math.pi * freq * t) + 0.28 * math.sin(2 * math.pi * freq * 3.0 * t)
        return v * math.exp(-6.0 * t) * min(1.0, t / 0.003)
    return fn


def bell(freq, dur=2.6):
    """campana FM: portadora + modulador que es decau ràpid."""
    def fn(t, i):
        mod = math.sin(2 * math.pi * freq * 2.76 * t) * math.exp(-7.0 * t)
        v = math.sin(2 * math.pi * freq * t + 1.4 * mod * 4.0)
        return v * math.exp(-1.5 * t) * min(1.0, t / 0.002)
    return fn


def shaker(dur=0.06):
    def fn(t, i):
        return (math.sin(t * 12345.678) % 1.0 * 2 - 1) * math.exp(-70 * t)
    return fn


def kick(dur=0.3):
    def fn(t, i):
        f = 92 - 44 * min(1.0, t / 0.06)
        return math.sin(2 * math.pi * f * t) * math.exp(-9.0 * t) * min(1.0, t / 0.003)
    return fn


# ── seccions (temps en segons) ──────────────────────────────────────────────
S_HOOK = (0.0, 5.6)
S_FRICTION = (5.6, 10.9)
S_REVEAL = (10.9, 13.6)
S_TALK = (13.6, 42.0)
S_DEMO = (42.0, 51.0)
S_PUBLISH = (51.0, 56.5)
S_CTA = (56.5, 65.0)



def pad_drone(buf, amp=0.13, pan=0.0):
    """Pad continu: les tres veus glissen d'un acord al següent sense cap tall.
    Un drone continu no té forats a les fronteres de temps (a diferència de
    tocar Notes i encollar-les, que deixava buits de mig segon)."""
    gl = math.sqrt((1.0 - pan) * 0.5) * 1.414
    gr = math.sqrt((1.0 + pan) * 0.5) * 1.414
    nbar = int(DUR / BAR) + 2
    seq = [CHORDS[b % 4] for b in range(nbar)]
    ph = [0.0, 0.0, 0.0]
    glide = 0.38
    for i in range(N - 1):
        t = i / SR
        bar = int(t / BAR)
        if bar >= nbar - 1:
            break
        nb = bar + 1
        k = min(1.0, (t - bar * BAR) / glide)
        for vi in range(3):
            f0 = PITCH[seq[bar]["pad"][vi]]
            f1 = PITCH[seq[nb]["pad"][vi]]
            f = f0 + (f1 - f0) * k
            ph[vi] += 2 * math.pi * f / SR
            v = 0.0
            for h in (1, 2, 3, 4):
                v += (1.0 / h) * math.sin(ph[vi] * h)
            v *= 0.26 * amp
            buf[i] += v * gl
            buf[i + 1] += v * gr


def build_music(vo_timings):
    random_state = [12345]

    def rnd():
        random_state[0] = (random_state[0] * 1103515245 + 12345) & 0x7FFFFFFF
        return random_state[0] / 0x7FFFFFFF

    pad = buf()
    bass = buf()
    mel = buf()
    mal = buf()
    shk = buf()
    bels = buf()
    spark = buf()

    # ── PAD: drone continu que glissa d'acord en acord
    pad_drone(pad, amp=0.40, pan=-0.18)

    # ── MELODIA (caixa de música): frase de 4 temps
    phrase = [  # (temps dins del cicle, durada en temps, nota)
        (0.0, 1.5, 0), (1.5, 0.5, 1), (2.0, 1.0, 2), (3.0, 1.0, 3),
        (4.0, 1.0, 0), (5.0, 1.0, 1), (6.0, 1.5, 2), (7.5, 0.5, 3),
        (8.0, 1.0, 0), (9.0, 1.0, 1), (10.0, 2.0, 2),
        (12.0, 1.0, 0), (13.0, 1.0, 1), (14.0, 1.0, 2), (15.0, 1.0, 3),
    ]
    mel_amp = 0.20
    for cyc, t0 in enumerate([0.0, 16.0, 26.0, 36.0, 46.0, 56.0]):
        for (bpos, blen, midx) in phrase:
            t = t0 + bpos * BEAT
            ch = CHORDS[int((t / BAR) % 4)]
            note = ch["meld"][midx % 4]
            oct_up = 1 if (cyc in (1, 3, 5) and bpos >= 8) else 0
            f = PITCH[note] * (2 if oct_up else 1)
            add(mel, t, blen * BEAT + 1.0, musicbox(f, 1.5), amp=mel_amp * (0.85 if cyc in (1, 2, 3, 4, 5) else 1),
                attack=0.003, pan=0.12)

    # ── BAIX: arrel a 1 i 3
    for b in range(26):
        t0 = b * BAR
        if t0 > DUR:
            break
        ch = CHORDS[b % 4]
        f = PITCH[ch["root"]]
        for beat, amp in ((0.0, 0.26), (2.0, 0.17)):
            t = t0 + beat * BEAT
            if t >= DUR:
                continue
            add(bass, t, BEAT * 1.9, bassnote(f, BEAT * 1.9), amp=amp, attack=0.02, release=0.35)

    # ── MALLER: 2 i 4
    for b in range(26):
        t0 = b * BAR
        if t0 > DUR:
            break
        ch = CHORDS[b % 4]
        for beat, note in ((1.0, ch["pad"][1]), (3.0, ch["pad"][2])):
            t = t0 + beat * BEAT
            if t >= DUR:
                continue
            add(mal, t, 0.5, mallet(PITCH[note] * 2), amp=0.075, attack=0.002, pan=0.28)

    # ── SHAKER: contrasTemps, molt suau
    for b in range(26):
        t0 = b * BAR
        if t0 > DUR:
            break
        for beat in range(4):
            t = t0 + (beat + 0.5) * BEAT
            if t >= DUR:
                continue
            add(shk, t, 0.07, shaker(), amp=0.032, attack=0.001, pan=-0.3)

    # ── CAMPANES: la revel·lació i la publicació
    for t0, notes in ((11.35, ["C6", "E6", "G6", "C7"]), (51.25, ["C6", "E6", "G6", "C7", "E7"])):
        for i, nn in enumerate(notes):
            add(bels, t0 + i * 0.11, 3.2, bell(PITCH[nn]), amp=0.16 - i * 0.018,
                attack=0.002, release=1.6, pan=-0.4 + (i / len(notes)) * 0.8)

    # ── BRILLANTS: un ping just abans de cada frase de veu (el ritme parla)
    ping_pool = ["C6", "E6", "G6", "A5", "C7", "D6", "G5", "E7"]
    for si, seg in enumerate(vo_timings):
        t = seg["in"] - 0.28
        if t < 0:
            continue
        hi = ping_pool[si % len(ping_pool)]
        add(spark, t, 2.0, musicbox(PITCH[hi], 1.7), amp=0.11, attack=0.003, release=1.2,
            pan=0.35 if seg["id"] in ("n5", "n7", "n9", "n12") else -0.35)
        add(spark, t + 0.10, 1.6, musicbox(PITCH[hi] * 1.5, 1.2), amp=0.05, attack=0.003)

    # ── CAPES
    # 1) ganxo: només pad + melodia molt escassa
    # 2) fricció: pad + melodia baixa i apagada (tensió)
    # 3) revel·lació: tot
    # 4) diàleg: llit prim (pad + brillants), MOLT espai per a la veu
    # 5) demo: construcció
    # 6) publicació: tot
    # 7) cartell final: resolució

    def env(section, t, a=0.35, r=0.5):
        s, e = section
        if t < s - a or t > e + r:
            return 0.0
        if t < s:
            return (t - (s - a)) / a
        if t > e:
            return max(0.0, (e + r - t) / r)
        return 1.0

    out = buf()
    for i in range(N):
        t = i / SR
        bar = int(t / BAR)
        ch = CHORDS[bar % 4]

        g_hook = env(S_HOOK, t, 0.6, 0.8)
        g_fric = env(S_FRICTION, t, 0.5, 0.7)
        g_rev = env(S_REVEAL, t, 0.05, 0.5)
        g_talk = env(S_TALK, t, 0.8, 1.0)
        g_demo = env(S_DEMO, t, 0.8, 0.7)
        g_pub = env(S_PUBLISH, t, 0.25, 0.6)
        g_cta = env(S_CTA, t, 0.7, 1.6)

        # el ganxo i la fricció tenen la melodia apagada i baixa
        low = 1.0 if (t >= S_REVEAL[0]) else 0.55
        v = 0.0
        v += pad[i] * (0.95 * g_hook + 0.68 * g_fric + 1.0 * g_rev + 1.05 * g_talk
                       + 0.90 * g_demo + 1.0 * g_pub + 1.0 * g_cta)
        v += mel[i] * (0.55 * g_hook + 0.22 * g_fric + 1.0 * g_rev + 0.62 * g_talk
                       + 1.0 * g_demo + 1.0 * g_pub + 1.0 * g_cta) * low
        v += bass[i] * (0.30 * g_hook + 0.30 * g_fric + 0.90 * g_rev + 0.32 * g_talk
                        + 1.0 * g_demo + 0.95 * g_pub + 0.85 * g_cta)
        v += mal[i] * (0.0 * g_hook + 0.0 * g_fric + 0.9 * g_rev + 0.34 * g_talk
                       + 1.0 * g_demo + 1.0 * g_pub + 0.7 * g_cta)
        v += shk[i] * (0.12 * g_hook + 0.35 * g_fric + 1.0 * g_rev + 0.14 * g_talk
                       + 0.85 * g_demo + 1.0 * g_pub + 0.5 * g_cta)
        v += bels[i] * (1.0 * g_rev + 1.0 * g_pub)
        v += spark[i] * (0.95 * g_talk + 0.7 * g_demo + 0.8 * g_pub + 0.8 * g_cta)
        out[i] = v

    # el ganxo i la fricció sonen apagats (fins de la revel·lació)
    for i in range(N):
        t = i / SR
        if t < S_REVEAL[0]:
            out[i] *= 0.72
        if t < 0.9:
            out[i] *= t / 0.9
        if t > DUR - 2.2:
            out[i] *= max(0.0, (DUR - t) / 2.2)

    return out


# ── escriptura ──────────────────────────────────────────────────────────────
def write_wav(path, left, right, peak=0.89):
    """Escriu un WAV estéreo INTERCALAT (L,R,L,R...). El mòdul wave espera
    mostres intercalats: escriure tots els L i després tots els R prodüïa un
    fitxer esbarrat que sonava completament esguerrat."""
    frames = array.array("h")
    for i in range(N):
        a = max(-1.0, min(1.0, left[i]))
        b = max(-1.0, min(1.0, right[i]))
        frames.append(int(a * 32767))
        frames.append(int(b * 32767))
    with wave_open(path) as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(frames.tobytes())


def write_wav_mono(path, data):
    """WAV mono 16 bit (per a la música seca: l'amplada estéreo la crea el
    filtrat de Haas a la codificació)."""
    frames = array.array("h")
    for i in range(N):
        v = max(-1.0, min(1.0, data[i]))
        frames.append(int(v * 32767))
    with wave_open(path) as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(frames.tobytes())


def wave_open(path):
    import wave
    return wave.open(path, "wb")


def read_wav(path):
    import wave
    with wave.open(path, "rb") as w:
        nch, sw, sr = w.getnchannels(), w.getsampwidth(), w.getframerate()
        raw = w.readframes(w.getnframes())
    data = array.array("h")
    data.frombytes(raw)
    if struct.pack("=h", 1) != struct.pack("<h", 1):
        data.byteswap()
    out = array.array("f", (v / 32768.0 for v in data))
    if nch == 2:
        mono = array.array("f", [0.0] * (len(out) // 2))
        for i in range(len(mono)):
            mono[i] = (out[2 * i] + out[2 * i + 1]) * 0.5
        out = mono
    return out, sr


def resample(data, sr, target):
    if sr == target:
        return data
    ratio = sr / target
    n = int(len(data) / ratio)
    out = array.array("f", bytes(4 * n))
    for i in range(n):
        j = i * ratio
        i0 = int(j)
        frac = j - i0
        i1 = min(i0 + 1, len(data) - 1)
        out[i] = data[i0] * (1 - frac) + data[i1] * frac
    return out


def main():
    timings = json.load(open(os.path.join(VO_DIR, "timings.json")))
    print("· sintetitzant la música…")
    dry = build_music(timings)

    m = max(abs(v) for v in dry) or 1.0
    k = 0.82 / m
    dry = array.array("f", [v * k for v in dry])
    write_wav_mono(os.path.join(AUDIO, "music_dry.wav"), dry)

    # reverberació + EQ + compressió amb ffmpeg
    print("· reverberació i masterització…")
    wet = os.path.join(AUDIO, "music.wav")
    subprocess.run([
        FF, "-y", "-hide_banner", "-loglevel", "error",
        "-i", os.path.join(AUDIO, "music_dry.wav"),
        "-af", ("aecho=0.88:0.72:37|61|89|131|187:0.42|0.34|0.27|0.21|0.16,haas=left_delay=9:right_delay=13:side_gain=1.1:level_out=0.95,"
                "lowpass=f=8200,highpass=f=48,"
                "equalizer=f=190:t=q:w=1.0:g=-2.0,"
                "equalizer=f=2400:t=q:w=1.2:g=2.2,"
                "acompressor=threshold=0.10:ratio=2.4:attack=12:release=220:makeup=1.35,"
                "alimiter=limit=0.94:attack=5:release=60"),
        "-ar", str(SR), "-ac", "2", wet,
    ], check=True)

    # ── veu + ducking
    print("· mesclant la veu…")
    voice = buf()
    for seg in timings:
        data, sr = read_wav(seg["file"])
        data = resample(data, sr, SR)
        # normalització ESTÀTICA (no dinàmica: un normalitzador dinàmic
        # amplifica el silenci dels extremts i crea soroll al voltant de cada
        # frase, que es fica per damunt del llit musical).
        pk = max((abs(v) for v in data), default=0.0)
        if pk > 0:
            k = 0.60 / pk
            data = array.array("f", [v * k for v in data])
        i0 = int(seg["in"] * SR)
        for i, v in enumerate(data):
            if i0 + i >= N:
                break
            voice[i0 + i] += v

    gain = array.array("f", [1.0]) * N
    for seg in timings:
        i0 = int(seg["in"] * SR)
        i1 = min(N, int((seg["in"] + seg["dur"]) * SR))
        ramp = int(0.34 * SR)
        floor = 0.25
        for i in range(max(0, i0 - ramp), min(N, i1 + ramp)):
            g = floor
            if i < i0:
                g = 1.0 + (floor - 1.0) * (i - (i0 - ramp)) / ramp
            elif i > i1:
                g = floor + (1.0 - floor) * min(1.0, (i - i1) / ramp)
            if g < gain[i]:
                gain[i] = g

    music, _ = read_wav(wet)
    music = resample(music, SR, SR)
    # pseudo-estéreo de Haas: la música (mono) s'obre amb un retard curt en un
    # canal; el senal «mid» (L+R) queda intacte i la veu, al centre.
    D = int(0.016 * SR)
    fl = buf()
    fr = buf()
    for i in range(N):
        mval = music[i] if i < len(music) else 0.0
        prev = music[i - D] if i >= D else 0.0
        mid = mval * gain[i] * 0.87
        side = (mval - prev) * 0.09 * gain[i]
        fl[i] = mid + side + voice[i] * 1.30
        fr[i] = mid - side + voice[i] * 1.30

    # limitador SUAU (tanh): un retall dur deformaria els pics de la veu
    for i in range(N):
        a, b = fl[i], fr[i]
        pk = max(abs(a), abs(b))
        if pk > 0.72:
            a = 0.97 * math.tanh(a / 0.97)
            b = 0.97 * math.tanh(b / 0.97)
            fl[i] = a
            fr[i] = b

    write_wav(os.path.join(AUDIO, "mix.wav"), fl, fr)
    print("  →", os.path.join(AUDIO, "mix.wav"))


if __name__ == "__main__":
    main()
