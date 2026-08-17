package net.cargomarket.omnideliv.courier.data.di

import android.content.Context
import androidx.room.Room
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import net.cargomarket.omnideliv.courier.data.db.CourierDb
import net.cargomarket.omnideliv.courier.data.db.OutboundDao
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object DatabaseModule {

    @Provides
    @Singleton
    fun db(@ApplicationContext context: Context): CourierDb =
        Room.databaseBuilder(context, CourierDb::class.java, "courier.db")
            // No `fallbackToDestructiveMigration`. This database holds the
            // outbound queue — deliveries the courier has recorded and the
            // server has not yet accepted — so dropping it on a schema change
            // would silently destroy money-moving work. A future migration is
            // written by hand or the upgrade fails loudly.
            .build()

    @Provides
    fun outboundDao(db: CourierDb): OutboundDao = db.outbound()
}
