package fr.jolannrt.aetherplay.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fr.jolannrt.aetherplay.bt.HidGamepad
import fr.jolannrt.aetherplay.net.Link
import fr.jolannrt.aetherplay.net.Server

private val Bg = Color(0xFF0D0B14)
private val Card = Color(0xFF17141F)
private val Accent = Color(0xFF8B5CF6)
private val Muted = Color(0xFFA19BB0)

@Composable
fun ConnectScreen(
    servers: List<Server>,
    link: Link.Status,
    bluetooth: HidGamepad.Status,
    onConnect: (Server) -> Unit,
    onPin: (String) -> Unit,
    onMakeVisible: () -> Unit,
    onRetryBluetooth: () -> Unit,
    onOpenPad: () -> Unit,
    btEvents: List<String> = emptyList(),
    btExtended: Boolean = true,
    onBtExtended: (Boolean) -> Unit = {},
) {
    Row(
        Modifier
            .fillMaxSize()
            .background(Bg)
            .padding(24.dp),
        horizontalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        // Wi-Fi: find AetherPlay, pair with the code it shows.
        Column(
            Modifier
                .weight(1f)
                .background(Card, RoundedCornerShape(16.dp))
                .padding(20.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            Header(Icons.Filled.Wifi, "Wi-Fi", "Trouve AetherPlay sur le réseau et adapte la manette au jeu lancé.")
            Spacer(Modifier.height(12.dp))
            when (link) {
                is Link.Status.Connected -> Line("Connecté à ${link.server}", Accent)
                is Link.Status.Connecting -> Line("Connexion…", Muted)
                is Link.Status.Failed -> Line(link.reason, Color(0xFFFF6B6B))
                is Link.Status.NeedPin -> PinEntry(onPin)
                Link.Status.Disconnected -> {}
            }
            Spacer(Modifier.height(8.dp))
            if (servers.isEmpty()) {
                Line("Recherche d'AetherPlay… (même réseau Wi-Fi que l'ordinateur, AetherPlay ouvert)", Muted)
                Spacer(Modifier.height(6.dp))
                Line(
                    "Réseau d'école ou public ? Les appareils y sont souvent isolés. Branche le téléphone en USB et active " +
                        "le partage de connexion USB (ou connecte l'ordinateur au point d'accès du téléphone), puis tape " +
                        "l'adresse affichée dans AetherPlay (Réglages > AetherPlay > Manette Android) ci-dessous. " +
                        "Le Bluetooth, lui, marche sur tous les réseaux.",
                    Color(0xFFFFB84D),
                )
            }
            servers.forEach { s ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(vertical = 4.dp)
                        .background(Color(0xFF221E2C), RoundedCornerShape(10.dp))
                        .clickable { onConnect(s) }
                        .padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Filled.Computer, null, tint = Accent)
                    Spacer(Modifier.width(10.dp))
                    Column {
                        Text(s.name, color = Color.White, fontWeight = FontWeight.SemiBold)
                        Text("${s.host}:${s.port}", color = Muted, fontSize = 12.sp)
                    }
                }
            }
            ManualEntry(onConnect)
        }

        // Bluetooth: the computer sees the phone as a wireless controller.
        Column(
            Modifier
                .weight(1f)
                .background(Card, RoundedCornerShape(16.dp))
                .padding(20.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            Header(Icons.Filled.Bluetooth, "Bluetooth", "Le téléphone devient une vraie manette pour les jeux, sur Windows, Mac et SteamOS.")
            Spacer(Modifier.height(12.dp))
            when (bluetooth) {
                is HidGamepad.Status.Connected -> Line("Manette connectée à ${bluetooth.host}", Accent)
                HidGamepad.Status.Ready -> {
                    Line("Prête. Première fois : rends le téléphone visible, puis sur l'ordinateur ouvre les réglages Bluetooth > Ajouter un appareil et choisis ce téléphone (il apparaît sous son propre nom).", Muted)
                    Spacer(Modifier.height(8.dp))
                    OutlinedButton(onClick = onMakeVisible) { Text("Rendre le téléphone visible") }
                }
                HidGamepad.Status.Starting -> Line("Démarrage…", Muted)
                is HidGamepad.Status.Unavailable -> {
                    Line("${bluetooth.reason}. La manette passera par le Wi-Fi.", Color(0xFFFFB84D))
                    Spacer(Modifier.height(8.dp))
                    OutlinedButton(onClick = onRetryBluetooth) { Text("Réessayer") }
                }
            }
            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Infos AetherPlay par Bluetooth", color = Color.White, fontSize = 14.sp)
                    Text(
                        "Console du jeu et numéro de joueur sans Wi-Fi. Si la liaison décroche, désactive : manette simple. " +
                            "Après un changement, supprime le téléphone des appareils Bluetooth de l'ordinateur et associe-le de nouveau.",
                        color = Muted, fontSize = 12.sp,
                    )
                }
                Switch(checked = btExtended, onCheckedChange = onBtExtended)
            }
            if (btEvents.isNotEmpty()) {
                Spacer(Modifier.height(10.dp))
                Text("Journal Bluetooth", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                btEvents.reversed().forEach { Text(it, color = Muted, fontSize = 11.sp) }
            }
            Spacer(Modifier.height(20.dp))
            Button(
                onClick = onOpenPad,
                colors = ButtonDefaults.buttonColors(containerColor = Accent),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Afficher la manette") }
        }
    }
}

@Composable
private fun Header(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, subtitle: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(icon, null, tint = Accent)
        Spacer(Modifier.width(8.dp))
        Text(title, color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
    }
    Text(subtitle, color = Muted, fontSize = 13.sp)
}

@Composable
private fun Line(text: String, color: Color) = Text(text, color = color, fontSize = 14.sp)

@Composable
private fun PinEntry(onPin: (String) -> Unit) {
    var pin by remember { mutableStateOf("") }
    Line("Tape le code affiché dans AetherPlay (Réglages > AetherPlay > Manette Android).", Color.White)
    Row(verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            pin, { pin = it.filter(Char::isDigit).take(6) },
            label = { Text("Code") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(8.dp))
        Button(onClick = { onPin(pin) }, enabled = pin.length == 6,
            colors = ButtonDefaults.buttonColors(containerColor = Accent)) { Text("Valider") }
    }
}

@Composable
private fun ManualEntry(onConnect: (Server) -> Unit) {
    var host by remember { mutableStateOf("") }
    Spacer(Modifier.height(12.dp))
    Row(verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            host, { host = it.trim() },
            label = { Text("Ou adresse IP de l'ordinateur") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(8.dp))
        OutlinedButton(onClick = {
            val (h, p) = host.split(':').let { it[0] to (it.getOrNull(1)?.toIntOrNull() ?: fr.jolannrt.aetherplay.net.Discovery.DEFAULT_PORT) }
            onConnect(Server(h, h, p))
        }, enabled = host.isNotEmpty()) { Text("Connexion") }
    }
}
