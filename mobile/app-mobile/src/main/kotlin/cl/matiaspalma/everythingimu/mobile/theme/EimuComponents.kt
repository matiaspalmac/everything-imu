package cl.matiaspalma.everythingimu.mobile.theme

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Section header mirroring --fg-section-header in apps/ui styles.css. */
@Composable
fun SectionHeader(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text.uppercase(),
        color = EimuPalette.FgMuted,
        fontWeight = FontWeight.SemiBold,
        fontSize = 11.sp,
        letterSpacing = 1.2.sp,
        modifier = modifier.padding(start = 4.dp, top = 4.dp, bottom = 2.dp),
    )
}

/** Panel card — radius-lg (14dp), BgPanel container. Matches desktop card surfaces. */
@Composable
fun EimuCard(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        colors = CardDefaults.cardColors(
            containerColor = EimuPalette.BgPanel,
            contentColor = EimuPalette.FgPrimary,
        ),
    ) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            content()
        }
    }
}

@Composable
fun CardTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleMedium, color = EimuPalette.FgPrimary)
}
