package io.logisticos.driver.navigation

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.navigation.NavController
import androidx.navigation.compose.currentBackStackEntryAsState

internal val NavCyan = Color(0xFF00E5FF)
internal val NavCanvas = Color(0xFF0A0E1A)

sealed class BottomTab(val route: String, val label: String, val icon: ImageVector) {
    object Home : BottomTab("home", "Home", Icons.Default.Home)
    object Route : BottomTab("route", "Route", Icons.Default.Place)
    object Scan : BottomTab("scan", "Scan", Icons.Default.QrCodeScanner)
    object Notifications : BottomTab("notifications", "Alerts", Icons.Default.Notifications)
    object Profile : BottomTab("profile", "Profile", Icons.Default.Person)
}

val bottomTabs = listOf(
    BottomTab.Home,
    BottomTab.Route,
    BottomTab.Scan,
    BottomTab.Notifications,
    BottomTab.Profile,
)

@Composable
fun BottomNavBar(navController: NavController, unreadCount: Int = 0) {
    val navBackStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = navBackStackEntry?.destination?.route

    NavigationBar(containerColor = NavCanvas, tonalElevation = 0.dp) {
        bottomTabs.forEach { tab ->
            NavigationBarItem(
                selected = currentRoute == tab.route,
                onClick = {
                    navController.navigate(tab.route) {
                        popUpTo(navController.graph.startDestinationId) { saveState = true }
                        launchSingleTop = true
                        restoreState = true
                    }
                },
                icon = {
                    if (tab is BottomTab.Notifications && unreadCount > 0) {
                        BadgedBox(badge = { Badge { Text("$unreadCount") } }) {
                            Icon(tab.icon, contentDescription = tab.label)
                        }
                    } else {
                        Icon(tab.icon, contentDescription = tab.label)
                    }
                },
                label = { Text(tab.label) },
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = NavCyan,
                    selectedTextColor = NavCyan,
                    unselectedIconColor = Color.White.copy(alpha = 0.4f),
                    unselectedTextColor = Color.White.copy(alpha = 0.4f),
                    indicatorColor = NavCyan.copy(alpha = 0.15f),
                ),
            )
        }
    }
}
