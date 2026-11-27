package io.logisticos.driver.move

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AccountBalanceWallet
import androidx.compose.material.icons.filled.FormatListBulleted
import androidx.compose.material.icons.filled.Inventory2
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController
import androidx.navigation.compose.currentBackStackEntryAsState
import io.logisticos.driver.core.designsystem.LocalMoveColors

private data class MoveTab(val route: String, val label: String, val icon: ImageVector)

/** The design's five tabs, over the app's existing destinations. */
private val MOVE_TABS = listOf(
    MoveTab("home", "LOADS", Icons.Filled.FormatListBulleted),
    MoveTab("route", "ROUTE", Icons.Filled.Map),
    MoveTab("hub", "STACK", Icons.Filled.Inventory2),
    MoveTab("compliance", "COMPLIANCE", Icons.Filled.VerifiedUser),
    MoveTab("earnings", "WALLET", Icons.Filled.AccountBalanceWallet),
)

/** Glove-sized (60 dp) tab bar. The handoff: do not shrink driver controls. */
@Composable
fun MoveBottomBar(navController: NavController) {
    val c = LocalMoveColors.current
    val entry by navController.currentBackStackEntryAsState()
    val current = entry?.destination?.route

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(c.ground)
            .navigationBarsPadding()
            .padding(horizontal = 12.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        MOVE_TABS.forEach { tab ->
            val selected = current == tab.route
            val shape = RoundedCornerShape(16.dp)
            Column(
                modifier = Modifier
                    .weight(1f)
                    .height(60.dp)
                    .clip(shape)
                    .background(if (selected) c.accentPanel else c.chip)
                    .border(1.dp, if (selected) c.accentBorder else c.hairline, shape)
                    .clickable(role = Role.Tab) {
                        navController.navigate(tab.route) {
                            popUpTo(navController.graph.startDestinationId) { inclusive = false }
                            launchSingleTop = true
                        }
                    },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Icon(tab.icon, contentDescription = null, tint = if (selected) c.accent else c.muted, modifier = Modifier.size(22.dp))
                Text(
                    text = tab.label,
                    color = if (selected) c.accent else c.muted,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                    letterSpacing = 0.4.sp,
                    maxLines = 1,
                    softWrap = false,
                )
            }
        }
    }
}
