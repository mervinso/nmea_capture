# NMEA Capture + DustDDS

Proyecto en Rust que:

1. **Escucha NMEA 0183 por UDP** y los imprime en consola (texto o JSON).
2. **(Opcional) Publica** esas líneas crudas en **Dust DDS** en el tópico `NMEA/Raw`.
3. Incluye un segundo binario `nmea_dds_sub` que **se suscribe** a `NMEA/Raw`, imprime **NDJSON** y, si detecta **RMC** o **GGA**, **parsea** y vuelve a publicar en tópicos tipados `NMEA/RMC` y `NMEA/GGA`.

> Diseñado para probar con **NemaStudio** en Windows 11; soporta WinSock con inicialización segura.

---

## Tabla de contenidos

- [Arquitectura](#arquitectura)
- [Requisitos](#requisitos)
- [Instalación](#instalación)
- [Ejecución rápida](#ejecución-rápida)
- [Parámetros CLI](#parámetros-cli)
- [Ejemplos](#ejemplos)
- [Configuración NemaStudio](#configuración-nemastudio)
- [Notas sobre DustDDS y Windows](#notas-sobre-dustdds-y-windows)
- [Estructura del repo](#estructura-del-repo)
- [Licencia](#licencia)

---

## Arquitectura

- **nmea-capture** (binario principal)
  - Socket UDP con Tokio.
  - Imprime a consola con o sin timestamp, y opcionalmente en **NDJSON**.
  - Si activas `--dds`, publica cada línea como un mensaje `RawSentence` en `NMEA/Raw`.

- **nmea_dds_sub** (segundo binario)
  - Se suscribe a `NMEA/Raw`.
  - Imprime NDJSON (`type=raw`).
  - Si la sentencia es `RMC` o `GGA`, la parsea a tipos **`Rmc`** / **`Gga`** y las **republica** en `NMEA/RMC` y `NMEA/GGA` (tipados con `#[derive(DdsType)]`).

---

## Requisitos

- **Rust** 1.75+ (stable) y **Cargo**  
- **Windows 11** (probado), Rust toolchain MSVC  
- **NemaStudio** (opcional) para generar NMEA por UDP  
- Conexión a Internet para resolver dependencias de crates

---

## Estructura del repo

```bash
nema_capture/
├─ Cargo.toml
├─ src/
│  ├─ main.rs              # bin nmea-capture
│  ├─ app.rs
│  ├─ cli.rs
│  ├─ nmea.rs
│  ├─ dds_types.rs         # RawSentence & Source (DdsType)
│  ├─ bin/
│  │  └─ nmea_dds_sub.rs   # segundo binario (DDS subscriber + republish RMC/GGA)
│  └─ net/
│     └─ udp.rs            # UdpReceiver con Tokio
└─ README.md
```

---

## Instalación

Clona el repo y entra en la carpeta:

```powershell
git clone https://github.com/mervinso/nema_capture.git
cd nema_capture
```
Compila una vez:

```powershell
cargo build --release
```

---

## Ejecución rápida

1) Arranca el capturador publicando a DDS

```powershell
$env:RUST_LOG = 'info,dust_dds=warn'   # opcional para silenciar logs de multicasts fallidos
cargo run --release -- `
  --bind 0.0.0.0:1100 `
  --mode unicast `
  --timestamp -v `
  --dds --dds-domain 0 --dds-topic-raw "NMEA/Raw"
```
Verás `INFO DDS enabled: domain=0 topic=NMEA/Raw`.
Deja esta ventana abierta recibiendo tráfico UDP (desde NemaStudio).

2) Abre otra ventana y corre el subscriber

```powershell
$env:RUST_LOG = 'info,dust_dds=warn'
cargo run --release --bin nmea_dds_sub -- `
  --domain 0 `
  --topic-raw "NMEA/Raw" `
  --topic-rmc "NMEA/RMC" `
  --topic-gga "NMEA/GGA"
```

En cuanto lleguen sentencias, verás NDJSON tipo `raw` y, para RMC/GGA, objetos `type="rmc"' / 'type="gga"` más logs `republished RMC/GGA`.

---

Parámetros CLI

`nmea-capture`

```rs
--bind <IP:PORT>        (def: 0.0.0.0:1100)
--mode <unicast|multicast> (v1 usa unicast)
--timestamp             (imprime timestamp en consola)
-v / -vv                (verbosidad; -q para quiet)
--json                  (salida en NDJSON)
--json-pretty           (multi-línea; requiere --json)
--dds                   (publica a DustDDS)
--dds-domain <id>       (def: 0)
--dds-topic-raw <name>  (def: "NMEA/Raw")
```

`nmea_dds_sub`

```rs
--domain <id>           (def: 0)
--topic-raw <name>      (def: "NMEA/Raw")
--topic-rmc <name>      (def: "NMEA/RMC")
--topic-gga <name>      (def: "NMEA/GGA")
```


---

## Ejemplos

Solo imprimir a consola en texto:

```powershell
cargo run --release -- --bind 0.0.0.0:1100 --mode unicast --timestamp -v
```

Imprimir en NDJSON:

```powershell
cargo run --release -- --json --bind 0.0.0.0:1100
```

Publicar a DDS y ver con el subscriber:

# Ventana A
```powershell
cargo run --release -- --dds --dds-domain 0 --dds-topic-raw "NMEA/Raw"
```

# Ventana B
```powershell
cargo run --release --bin nmea_dds_sub -- --domain 0 --topic-raw "NMEA/Raw"
```

---

## Configuración NemaStudio

- UDP: Remote IP 127.0.0.1, Remote Port 1100, Local Port 1100

- Modo: Unicast (para multicast no se unen a grupos en esta v1).

- Enviar frases (p. ej. RMC/GGA) a 1 Hz para pruebas.

---

