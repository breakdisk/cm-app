package io.logisticos.driver

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import dagger.hilt.android.AndroidEntryPoint
import io.logisticos.driver.security.RootChecker
import io.logisticos.driver.ui.theme.DriverAppTheme
import io.logisticos.driver.navigation.AppNavGraph

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val rootChecker = RootChecker(this)
        if (rootChecker.check()) {
            android.util.Log.w("Security", "Rooted device detected — flagging for audit")
        }
        setContent {
            DriverAppTheme {
                AppNavGraph()
            }
        }
    }
}
