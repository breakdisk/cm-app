package io.logisticos.driver

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import dagger.hilt.android.AndroidEntryPoint
import io.logisticos.driver.core.database.worker.OutboundSyncWorker
import io.logisticos.driver.navigation.AppNavGraph
import io.logisticos.driver.security.RootChecker
import io.logisticos.driver.ui.theme.DriverAppTheme
import javax.inject.Inject

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    @Inject lateinit var rootChecker: RootChecker

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val isRooted = rootChecker.check()
        OutboundSyncWorker.schedule(applicationContext)
        setContent {
            DriverAppTheme {
                AppNavGraph()
                if (isRooted) {
                    var dismissed by rememberSaveable { mutableStateOf(false) }
                    if (!dismissed) {
                        AlertDialog(
                            onDismissRequest = { dismissed = true },
                            title = { Text("Security Warning") },
                            text = {
                                Text(
                                    "This device appears to be rooted. " +
                                    "Using the driver app on a rooted device may violate company policy " +
                                    "and could expose sensitive delivery data. " +
                                    "Please contact your supervisor."
                                )
                            },
                            confirmButton = {
                                TextButton(onClick = { dismissed = true }) {
                                    Text("I Understand")
                                }
                            }
                        )
                    }
                }
            }
        }
    }
}
