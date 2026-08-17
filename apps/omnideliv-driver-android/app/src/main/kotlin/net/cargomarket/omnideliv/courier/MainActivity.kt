package net.cargomarket.omnideliv.courier

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import dagger.hilt.android.AndroidEntryPoint
import net.cargomarket.omnideliv.courier.ui.CourierTheme
import net.cargomarket.omnideliv.courier.ui.Tokens

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            CourierTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Tokens.Base,
                ) {
                    CourierNavHost()
                }
            }
        }
    }
}
